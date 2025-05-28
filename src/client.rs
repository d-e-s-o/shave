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
/// See https://gist.github.com/rihardn/47b8e6170dc8f57a998c90b12a3e01bb
static CHROME_ARGS: [&str; 55] = [
  // All pop-ups and calls to window.open will fail.
  "--block-new-web-contents",
  // Disable various background network services, including extension
  // updating,safe browsing service, upgrade detector, translate, UMA.
  "--disable-background-networking",
  // Disable timers being throttled in background pages/tabs.
  "--disable-background-timer-throttling",
  // Normally, Chrome will treat a 'foreground' tab instead as
  // backgrounded if the surrounding window is occluded (aka visually
  // covered) by another window. This flag disables that.
  "--disable-backgrounding-occluded-windows",
  "--disable-blink-features",
  "--disable-blink-features=AutomationControlled",
  // Disable crashdump collection.
  "--disable-breakpad",
  "--disable-browser-side-navigation",
  // Disables client-side phishing detection.
  "--disable-client-side-phishing-detection",
  // Disable some built-in extensions that aren't affected by
  // `--disable-extensions`.
  "--disable-component-extensions-with-background-pages",
  // Don't update the browser 'components' listed at
  // chrome://components/.
  "--disable-component-update",
  // Disable installation of default apps.
  "--disable-default-apps",
  // Disables Domain Reliability Monitoring, which tracks whether the
  // browser has difficulty contacting Google-owned sites and uploads
  // reports to Google.
  "--disable-domain-reliability",
  // Disable all chrome extensions.
  "--disable-extensions",
  // Disallow opening links in external applications.
  "--disable-external-intent-requests",
  // Disables (mostly for hermetic testing) autofill server communication.
  "--disable-features=AutofillServerCommunication",
  // Disable the feature of: Calculate window occlusion on Windows will
  // be used in the future to throttle and potentially unload foreground
  // tabs in occluded windows.
  "--disable-features=CalculateNativeWinOcclusion",
  // Hide toolbar button that opens dialog for controlling media
  // sessions.
  "--disable-features=GlobalMediaControls",
  // Disables an improved UI for third-party cookie blocking in
  // incognito mode.
  "--disable-features=ImprovedCookieControls",
  // Disables the Discover feed on NTP.
  "--disable-features=InterestFeedContentSuggestions",
  // Disable the Chrome Media Router which creates some background
  // network activity to discover castable targets.
  "--disable-features=MediaRouter",
  // Disable the Chrome Optimization Guide and networking with its
  // service API.
  "--disable-features=OptimizationHints",
  "--disable-features=site-per-process",
  // Disables Chrome translation, both the manual option and the popup
  // prompt when a page with differing language is detected.
  "--disable-features=Translate",
  "--disable-features=TranslateUI",
  "--disable-gpu",
  // Suppresses hang monitor dialogs in renderer processes. This flag
  // may allow slow unload handlers on a page to prevent the tab from
  // closing.
  "--disable-hang-monitor",
  // Some javascript functions can be used to flood the browser process
  // with IPC. By default, protection is on to limit the number of IPC
  // sent to 10 per second per frame. This flag disables it.
  "--disable-ipc-flooding-protection",
  // Disables the Web Notification and the Push APIs.
  "--disable-notifications",
  // Make the values returned to window.performance.memory bucketized
  // and updated less frequently.
  "--disable-precise-memory-info",
  // Reloading a page that came from a POST normally prompts the user.
  "--disable-prompt-on-repost",
  // This disables non-foreground tabs from getting a lower process
  // priority This doesn't (on its own) affect timers or painting
  // behavior.
  "--disable-renderer-backgrounding",
  "--disable-setuid-sandbox",
  "--disable-site-isolation-trials",
  // Disable syncing to a Google account.
  "--disable-sync",
  "--disable-threaded-animation",
  // Disable multithreaded GPU compositing of web content.
  //
  // Note: This flag seems to have the potential to cause total havoc,
  //       with nothing being displayed or done at all. Probably not
  //       wise to enable it.
  //"--disable-threaded-compositing",
  "--disable-threaded-scrolling",
  // Disable a few things considered not appropriate for automation.
  "--enable-automation",
  "--enable-features=NetworkService,NetworkServiceInProcess",
  // Logging behavior slightly more appropriate for a server-type process.
  "--enable-logging=stderr",
  // New, native Headless mode.
  "--headless=new",
  // Hide scrollbars from screenshots.
  "--hide-scrollbars",
  "--incognito",
  "--lang=en_US",
  // 0 means INFO and higher. 2 is the most verbose.
  "--log-level=0",
  // Disable reporting to UMA, but allows for collection.
  "--metrics-recording-only",
  // Mute any audio.
  "--mute-audio",
  // Disable the default browser check, do not prompt to set it as such.
  "--no-default-browser-check",
  // Skip first run wizards.
  "--no-first-run",
  // Don't send hyperlink auditing pings.
  "--no-pings",
  // `--no-sandbox` is required in case we are running as root and we do
  // not want to impose arbitrary restrictions.
  "--no-sandbox",
  // Disables the service process from adding itself as an autorun
  // process. This does not delete existing autorun registrations, it
  // just prevents the service from registering a new one.
  "--no-service-autorun",
  // Avoid potential instability of using Gnome Keyring or KDE wallet.
  "--password-store=basic",
  // Runs the renderer and plugins in the same process as the browser.
  "--single-process",
  // Use mock keychain on Mac to prevent the blocking permissions
  // dialog.
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
