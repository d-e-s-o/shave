// Copyright (C) 2024-2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::net::SocketAddr;
use std::path::Path;

use anyhow::Context as _;
use anyhow::Result;

use chromedriver_launch::Chromedriver;

use fantoccini::wd::Capabilities;
use fantoccini::Client as WebdriverClient;
use fantoccini::ClientBuilder;
use fantoccini::Locator;

use hyper_util::client::legacy::connect::HttpConnector;

use serde_json::json;

use tempfile::TempDir;


/// A type encompassing options for capturing a screenshot.
#[derive(Clone, Debug, Default)]
pub struct ScreenshotOpts {
  /// The dimensions of the window to configure, in pixels.
  pub window_size: Option<(usize, usize)>,
  /// The CSS selector describing an element to wait for before
  /// capturing a screenshot.
  pub await_selector: Option<String>,
  /// The selector identifying one or more elements to remove before the
  /// screenshot is captured.
  pub remove_selector: Option<String>,
  /// The selector describing the element to screenshot.
  pub selector: Option<String>,
  /// The type is non-exhaustive and open to extension.
  #[doc(hidden)]
  pub _non_exhaustive: (),
}


/// Arguments to be passed to Chrome by default.
/// See https://github.com/puppeteer/puppeteer/blob/4846b8723cf20d3551c0d755df394cc5e0c82a94/src/node/Launcher.ts#L157
static CHROME_ARGS: [&str; 33] = [
  "--disable-background-networking",
  "--disable-background-timer-throttling",
  "--disable-backgrounding-occluded-windows",
  "--disable-blink-features",
  "--disable-blink-features=AutomationControlled",
  "--disable-breakpad",
  "--disable-browser-side-navigation",
  "--disable-client-side-phishing-detection",
  "--disable-component-extensions-with-background-pages",
  "--disable-default-apps",
  "--disable-dev-shm-usage",
  "--disable-extensions",
  "--disable-features=TranslateUI",
  "--disable-gpu",
  "--disable-hang-monitor",
  "--disable-ipc-flooding-protection",
  "--disable-popup-blocking",
  "--disable-prompt-on-repost",
  "--disable-renderer-backgrounding",
  "--disable-setuid-sandbox",
  "--disable-sync",
  "--enable-automation",
  "--enable-features=NetworkService,NetworkServiceInProcess",
  "--headless",
  "--hide-scrollbars",
  "--incognito",
  "--lang=en_US",
  "--metrics-recording-only",
  "--mute-audio",
  "--no-first-run",
  // `--no-sandbox` is required in case we are running as root and we do
  // not want to impose arbitrary restrictions.
  "--no-sandbox",
  "--password-store=basic",
  "--use-mock-keychain",
];


/// A builder for configurable construction of [`Client`] objects.
#[derive(Debug, Default)]
pub struct Builder {
  /// The user agent to use.
  user_agent: Option<String>,
}

impl Builder {
  /// Set/reset the user agent to use.
  pub fn set_user_agent(mut self, user_agent: Option<String>) -> Self {
    self.user_agent = user_agent;
    self
  }

  async fn connect(&self, addr: SocketAddr, data_dir: &Path) -> Result<WebdriverClient> {
    let webdriver_url = format!("http://{addr}");
    let mut args = Vec::from(CHROME_ARGS);
    let data_dir_arg = format!("--user-data-dir={}", data_dir.display());
    let () = args.push(&data_dir_arg);

    let user_agent_arg;
    if let Some(user_agent) = &self.user_agent {
      user_agent_arg = format!("--user-agent={user_agent}");
      let () = args.push(&user_agent_arg);
    }

    let opts = json!({"args": args});
    let mut capabilities = Capabilities::new();
    let _val = capabilities.insert("goog:chromeOptions".to_string(), opts);

    let client = ClientBuilder::new(HttpConnector::new())
      .capabilities(capabilities)
      .connect(&webdriver_url)
      .await
      .with_context(|| format!("failed to connect to {webdriver_url}"))?;

    Ok(client)
  }

  /// Create the [`Client`] object.
  pub async fn build(self) -> Result<Client> {
    let chromedriver = Chromedriver::launch()?;
    let data_dir = TempDir::new().context("failed to create temporary directory")?;
    let webdriver = self
      .connect(chromedriver.socket_addr(), data_dir.path())
      .await?;
    let slf = Client {
      chromedriver,
      webdriver,
      data_dir,
    };
    Ok(slf)
  }
}


/// A client for shaving data of websites.
pub struct Client {
  /// The Chromedriver process.
  chromedriver: Chromedriver,
  /// The WebDriver client object (communicating with the process).
  webdriver: WebdriverClient,
  /// The data directory for the Chrome instance.
  data_dir: TempDir,
}

impl Client {
  /// Instantiate a new `Client`.
  pub async fn new() -> Result<Self> {
    Builder::default().build().await
  }

  /// Retrieve a [`Builder`] object for configurable construction of a
  /// [`Client`].
  pub fn builder() -> Builder {
    Builder::default()
  }

  /// Destroy the `Client` object, freeing up all resources.
  #[inline]
  pub async fn destroy(self) -> Result<()> {
    let () = self
      .webdriver
      .close()
      .await
      .context("failed to close webdriver client connection")?;

    let () = self
      .chromedriver
      .destroy()
      .context("failed to shut down chromedriver process")?;

    let path = self.data_dir.path().to_path_buf();
    let () = self
      .data_dir
      .close()
      .with_context(|| format!("failed to remove data directory `{}`", path.display()))?;

    Ok(())
  }

  /// Capture a screenshot in the form of a PNG image.
  pub async fn screenshot(&mut self, url: &str, opts: &ScreenshotOpts) -> Result<Vec<u8>> {
    let ScreenshotOpts {
      window_size,
      await_selector,
      remove_selector,
      selector,
      _non_exhaustive: (),
    } = opts;

    let (w, h) = window_size.unwrap_or((3840, 2160));
    let () = self.webdriver.set_window_size(w as _, h as _).await?;

    let () = self
      .webdriver
      .goto(url)
      .await
      .with_context(|| format!("failed to navigate to {url}"))?;

    if let Some(await_selector) = await_selector {
      let _elem = self
        .webdriver
        .wait()
        .for_element(Locator::Css(await_selector))
        .await
        .with_context(|| format!("failed to await `{await_selector}`"))?;
    }

    if let Some(remove_selector) = remove_selector {
      // Definitely vulnerable to code injection here ¯\_(°ペ)_/¯
      let js = format!(
        r#"
        document
          .querySelectorAll('{remove_selector}')
          .forEach(function(node){{node.parentNode.removeChild(node)}})
      "#
      );
      let _output = self
        .webdriver
        .execute(&js, Vec::new())
        .await
        .with_context(|| format!("failed to remove `{remove_selector}`"))?;
    }

    let screenshot = if let Some(selector) = selector {
      let element = self
        .webdriver
        .find(Locator::Css(selector))
        .await
        .with_context(|| format!("failed to find `{selector}`"))?;

      let screenshot = element
        .screenshot()
        .await
        .with_context(|| format!("failed to screenshot `{selector}`"))?;

      screenshot
    } else {
      let screenshot = self
        .webdriver
        .screenshot()
        .await
        .with_context(|| format!("failed to screenshot `{url}`"))?;

      screenshot
    };

    Ok(screenshot)
  }
}
