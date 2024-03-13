// Copyright (C) 2024 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(
  clippy::collapsible_if,
  clippy::fn_to_numeric_cast,
  clippy::let_and_return,
  clippy::let_unit_value
)]

mod socket;
mod tcp;

use std::collections::HashSet;
use std::future::Future;
use std::net::Ipv4Addr;
use std::process::Stdio;
use std::time::Duration;

use anyhow::bail;
use anyhow::Context as _;
use anyhow::Result;

use hyper::client::HttpConnector;

use serde_json::json;

use fantoccini::wd::Capabilities;
use fantoccini::Client as WebdriverClient;
use fantoccini::ClientBuilder;
use fantoccini::Locator;

use tempfile::TempDir;

use tokio::process;
use tokio::process::Child;
use tokio::time::sleep;
use tokio::time::Instant;


/// The timeout used when searching for a bound local port.
const PORT_FIND_TIMEOUT: Duration = Duration::from_secs(30);


/// Arguments to be passed to Chrome by default.
/// See https://github.com/puppeteer/puppeteer/blob/4846b8723cf20d3551c0d755df394cc5e0c82a94/src/node/Launcher.ts#L157
static CHROME_ARGS: [&str; 29] = [
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
];


async fn find_localhost_port(pid: u32) -> Result<u16> {
  let start = Instant::now();

  // Wait for the driver process to bind to a local host address.
  let port = loop {
    let inodes = socket::socket_inodes(pid)?.collect::<Result<HashSet<_>>>()?;
    let result = tcp::parse(pid)?.find(|result| match result {
      Ok(entry) => {
        if inodes.contains(&entry.inode) {
          entry.addr == Ipv4Addr::LOCALHOST
        } else {
          false
        }
      },
      Err(_) => true,
    });
    match result {
      None => {
        if start.elapsed() >= PORT_FIND_TIMEOUT {
          bail!("failed to find local host port for process {pid}");
        }
        sleep(Duration::from_millis(1)).await
      },
      Some(result) => {
        break result
          .context("failed to find localhost proc tcp entry")?
          .port
      },
    }
  };

  Ok(port)
}

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
    .arg("--port=0")
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .with_context(|| format!("failed to launch `{chromium}` instance"))?;
  let pid = webdriver
    .id()
    .with_context(|| format!("spawned `{chromium}` process has no PID"))?;

  with_child(webdriver, || async {
    let port = find_localhost_port(pid).await?;
    let webdriver_url = format!("http://127.0.0.1:{port}");
    let data_dir = TempDir::new().context("failed to create temporary directory")?;
    let mut args = Vec::from(CHROME_ARGS);
    let data_dir_arg = format!("--user-data-dir={}", data_dir.path().display());
    let () = args.push(&data_dir_arg);
    let opts = json!({"args": args});
    let mut capabilities = Capabilities::new();
    let _val = capabilities.insert("goog:chromeOptions".to_string(), opts);

    let client = ClientBuilder::new(HttpConnector::new())
      .capabilities(capabilities)
      .connect(&webdriver_url)
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


#[cfg(test)]
mod tests {
  use super::*;

  use std::process;

  use tokio::join;
  use tokio::net::TcpListener;
  use tokio::test;
  use tokio::time::advance;
  use tokio::time::pause;


  /// Check that we can find a bound port on localhost.
  #[test]
  async fn localhost_port_finding() {
    {
      let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
      let addr = listener.local_addr().unwrap();
      let port = find_localhost_port(process::id()).await.unwrap();
      assert_eq!(port, addr.port());
    }

    // Test timeout in here as well, to make sure that we don't
    // accidentally conflict with the bind above from another test.

    {
      let () = pause();

      let fnd = find_localhost_port(process::id());
      let adv = advance(PORT_FIND_TIMEOUT);
      // NB: Tokio's `join` macro does not explicitly state the order in
      //     which futures are polled. This code relies on the `fnd`
      //     future being polled first, so that we have the start time
      //     set *before* advancing the time. In current versions of
      //     Tokio (1.36) this seems to always be the case.
      let (result, ()) = join!(fnd, adv);

      let err = result.unwrap_err();
      assert!(
        err
          .to_string()
          .contains("failed to find local host port for process"),
        "{err}"
      );
    }
  }
}
