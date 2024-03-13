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

use tokio::fs::write;
use tokio::io::stdout;
use tokio::io::AsyncWriteExt as _;

use crate::args::Args;
use crate::args::Command;
use crate::args::Output;
use crate::args::Screenshot;


/// Handler for the `screenshot` command.
async fn screenshot(screenshot: Screenshot) -> Result<()> {
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

  let screenshot = shave::screenshot(&url, &opts).await?;

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
      _ => return Err(err).context("failed to parse program arguments"),
    },
  };

  match args.command {
    Command::Screenshot(screenshot) => self::screenshot(screenshot).await,
  }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
  run(args_os()).await
}
