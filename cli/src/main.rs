// Copyright (C) 2024 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

mod args;

use std::env::args_os;
use std::ffi::OsString;
use std::path::PathBuf;

use anyhow::Context as _;
use anyhow::Result;

use clap::Parser as _;

use chrono::offset::Local;

use shave::Client;

use tokio::fs::write;
use tokio::io::stdout;
use tokio::io::AsyncWriteExt as _;

use crate::args::Args;
use crate::args::Command;
use crate::args::Output;
use crate::args::Screenshot;


/// Handler for the `screenshot` command.
async fn screenshot(mut client: Client, screenshot: Screenshot) -> Result<()> {
  let Screenshot {
    url,
    await_selector,
    selector,
    output,
  } = screenshot;

  let opts = shave::ScreenshotOpts {
    await_selector,
    selector,
    _non_exhaustive: (),
  };

  let screenshot = client
    .screenshot(&url, &opts)
    .await
    .with_context(|| format!("failed to capture screenshot of `{url}`"))?;
  let () = client
    .destroy()
    .await
    .context("failed to destroy `shave` client")?;

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
async fn run<A, T>(args: A) -> Result<()>
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
      _ => return Err(err.into()),
    },
  };

  let client = shave::Client::builder()
    .set_user_agent(args.user_agent)
    .build()
    .await
    .context("failed to instantiate `shave` client")?;

  match args.command {
    Command::Screenshot(screenshot) => self::screenshot(client, screenshot).await,
  }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
  run(args_os()).await
}
