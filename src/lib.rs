// Copyright (C) 2024 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(
  clippy::collapsible_if,
  clippy::fn_to_numeric_cast,
  clippy::let_and_return,
  clippy::let_unit_value
)]

use std::future::Future;
use std::io;
use std::process::Stdio;

use anyhow::Context as _;
use anyhow::Result;

use hyper::client::HttpConnector;

use serde_json::json;

use fantoccini::wd::Capabilities;
use fantoccini::Client as WebdriverClient;
use fantoccini::ClientBuilder;
use fantoccini::Locator;

use tokio::net::TcpSocket;
use tokio::process;
use tokio::process::Child;


/// Arguments to be passed to Chrome by default.
/// See https://github.com/puppeteer/puppeteer/blob/4846b8723cf20d3551c0d755df394cc5e0c82a94/src/node/Launcher.ts#L157
static CHROME_ARGS: [&str; 30] = [
  "--enable-features=NetworkService,NetworkServiceInProcess",
  "--disable-background-networking",
  "--disable-background-timer-throttling",
  "--disable-backgrounding-occluded-windows",
  "--disable-breakpad",
  "--disable-client-side-phishing-detection",
  "--disable-component-extensions-with-background-pages",
  "--disable-default-apps",
  "--disable-dev-shm-usage",
  "--disable-extensions",
  "--disable-features=TranslateUI",
  "--disable-hang-monitor",
  "--disable-ipc-flooding-protection",
  "--disable-popup-blocking",
  "--disable-prompt-on-repost",
  "--disable-renderer-backgrounding",
  "--disable-sync",
  "--force-color-profile=srgb",
  "--metrics-recording-only",
  "--no-first-run",
  "--enable-automation",
  "--password-store=basic",
  "--use-mock-keychain",
  "--enable-blink-features=IdleDetection",
  "--headless",
  "--hide-scrollbars",
  "--mute-audio",
  "--incognito",
  "--lang=en_US",
  // TODO: This should probably be made different for each session?
  "--user-data-dir=/tmp/chromedriver",
];


async fn with_child<F, T, Fut>(mut child: Child, f: F) -> Result<T>
where
  F: FnOnce() -> Fut,
  Fut: Future<Output = Result<T>>,
{
  let result = f().await;

  let () = child
    .kill()
    .await
    .context("failed to shut down webdriver process")?;

  result
}

async fn with_fantoccini<F, T, Fut>(f: F) -> Result<T>
where
  F: FnOnce(WebdriverClient) -> Fut,
  Fut: Future<Output = Result<T>>,
{
  let chromium = "chromedriver";
  let webdriver = process::Command::new(chromium)
    .arg("--port=9515")
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .with_context(|| format!("failed to launch `{chromium}` instance"))?;

  with_child(webdriver, || async {
    let addr = "127.0.0.1:9515"
      .parse()
      .context("failed to parse `127.0.0.1:9515` as socket address")?;
    // Error reporting from `fantoccini` and `hyper` is just braindead
    // and there is no sensible way to detect a refused connection. So
    // spin up a socket here to wait until the server is up and running.
    let () = loop {
      let socket = TcpSocket::new_v4()?;
      match socket.connect(addr).await {
        Err(err) if err.kind() == io::ErrorKind::ConnectionRefused => (),
        // Ignore other errors as well as success. Ultimately we let the
        // call below report the result.
        _ => break,
      }
    };

    let webdriver_url = "http://127.0.0.1:9515";
    let opts = json!({"args": CHROME_ARGS});
    let mut capabilities = Capabilities::new();
    let _val = capabilities.insert("goog:chromeOptions".to_string(), opts);

    let client = ClientBuilder::new(HttpConnector::new())
      .capabilities(capabilities)
      .connect(webdriver_url)
      .await
      .with_context(|| format!("failed to connect to {webdriver_url}"))?;

    f(client).await
  })
  .await
}


/// A type capturing options for capturing a screenshot.
#[derive(Clone, Debug, Default)]
pub struct ScreenshotOpts {
  /// The CSS selector describing an element to wait for before
  /// capturing a screenshot.
  pub await_selector: Option<String>,
  /// The selector describing the element to screenshot.
  pub selector: Option<String>,
  /// The type is non-exhaustive and open to extension.
  #[doc(hidden)]
  pub _non_exhaustive: (),
}


/// Capture a screenshot in the form of a PNG image.
pub async fn screenshot(url: &str, opts: &ScreenshotOpts) -> Result<Vec<u8>> {
  let ScreenshotOpts {
    await_selector,
    selector,
    _non_exhaustive: (),
  } = opts;

  let screenshot = with_fantoccini(|client| async {
    let () = client.set_window_size(3840, 2160).await?;

    let () = client
      .goto(url)
      .await
      .with_context(|| format!("failed to navigate to {url}"))?;

    if let Some(await_selector) = await_selector {
      let _elem = client
        .wait()
        .for_element(Locator::Css(await_selector))
        .await
        .with_context(|| format!("failed to await `{await_selector}`"))?;
    }

    let screenshot = if let Some(selector) = selector {
      let element = client
        .find(Locator::Css(selector))
        .await
        .with_context(|| format!("failed to find `{selector}`"))?;

      let screenshot = element
        .screenshot()
        .await
        .with_context(|| format!("failed to screenshot `{selector}`"))?;

      screenshot
    } else {
      let screenshot = client
        .screenshot()
        .await
        .with_context(|| format!("failed to screenshot `{url}`"))?;

      screenshot
    };

    let () = client
      .close()
      .await
      .context("failed to close webdriver client connection")?;

    Ok(screenshot)
  })
  .await?;

  Ok(screenshot)
}
