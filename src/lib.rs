// Copyright (C) 2024 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(
  clippy::collapsible_if,
  clippy::fn_to_numeric_cast,
  clippy::let_and_return,
  clippy::let_unit_value
)]

mod args;

use std::ffi::OsString;
use std::future::Future;
use std::io;
use std::path::PathBuf;
use std::process::Stdio;

use anyhow::Context as _;
use anyhow::Result;

use clap::Parser as _;

use chrono::offset::Local;

use hyper::client::HttpConnector;

use serde_json::json;

use fantoccini::wd::Capabilities;
use fantoccini::Client as WebdriverClient;
use fantoccini::ClientBuilder;
use fantoccini::Locator;

use tokio::fs::write;
use tokio::io::stdout;
use tokio::io::AsyncWriteExt as _;
use tokio::net::TcpSocket;
use tokio::process;
use tokio::process::Child;

use crate::args::Args;
use crate::args::Command;
use crate::args::Output;
use crate::args::Screenshot;


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

/// Handler for the `screenshot` command.
async fn screenshot(screenshot: Screenshot) -> Result<()> {
  let Screenshot {
    url,
    await_selector,
    selector,
    output,
  } = screenshot;

  let screenshot = with_fantoccini(|client| async {
    let () = client.set_window_size(3840, 2160).await?;

    let () = client
      .goto(&url)
      .await
      .with_context(|| format!("failed to navigate to {url}"))?;

    if let Some(await_selector) = await_selector {
      let _elem = client
        .wait()
        .for_element(Locator::Css(&await_selector))
        .await
        .with_context(|| format!("failed to await `{await_selector}`"))?;
    }

    let screenshot = if let Some(selector) = selector {
      let element = client
        .find(Locator::Css(&selector))
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

  let output = output.unwrap_or_else(|| {
    let now = Local::now();
    let path = PathBuf::from(format!("screenshot-{}.png", now.format("%+")));
    Output::Path(path)
  });

  match output {
    Output::Path(path) => write(&path, &screenshot)
      .await
      .with_context(|| format!("failed to write screenshot data to `{}`", path.display())),
    Output::Stdout => stdout()
      .write_all(&screenshot)
      .await
      .context("failed to write screenshot data to stdout"),
  }
}

/// Run the program and report errors, if any.
pub async fn run<A, T>(args: A) -> Result<()>
where
  A: IntoIterator<Item = T>,
  T: Into<OsString> + Clone,
{
  let args = match Args::try_parse_from(args) {
    Ok(args) => args,
    Err(err) => match err.kind() {
      clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion => {
        print!("{}", err);
        return Ok(())
      },
      _ => return Err(err).context("failed to parse program arguments"),
    },
  };

  match args.command {
    Command::Screenshot(screenshot) => self::screenshot(screenshot).await,
  }
}
