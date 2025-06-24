// Copyright (C) 2024-2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

mod args;

use std::env::args_os;
use std::ffi::OsString;
use std::io::stdin;
use std::path::PathBuf;

use anyhow::Context as _;
use anyhow::Error;
use anyhow::Result;

use clap::Parser as _;

use chrono::offset::Local;

use shave::Client;

use tokio::fs::write;
use tokio::io::stdout;
use tokio::io::AsyncWriteExt as _;
use tokio::task::spawn_blocking;

use crate::args::Args;
use crate::args::Command;
use crate::args::Launch;
use crate::args::Output;
use crate::args::Screenshot;


/// Handler for the `screenshot` command.
async fn screenshot(client: &mut Client, screenshot: Screenshot) -> Result<()> {
  let Screenshot {
    url,
    window_size,
    await_selector,
    remove_selector,
    selector,
    output,
  } = screenshot;

  let opts = shave::ScreenshotOpts {
    window_size,
    await_selector,
    remove_selector,
    selector,
    _non_exhaustive: (),
  };

  let screenshot = client
    .screenshot(&url, &opts)
    .await
    .with_context(|| format!("failed to capture screenshot of `{url}`"))?;
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

/// Handler for the `launch` command.
async fn launch(_client: &mut Client, launch: Launch) -> Result<()> {
  let Launch {} = launch;
  let () = spawn_blocking(|| {
    let mut buffer = String::new();
    let _count = stdin().read_line(&mut buffer)?;
    Result::<_, Error>::Ok(())
  })
  .await
  .unwrap()
  .context("failed to wait for user input")?;
  Ok(())
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

  let mut client = shave::Client::builder()
    .set_user_agent(args.user_agent)
    .set_headless(!matches!(args.command, Command::Launch(..)))
    .build()
    .await
    .context("failed to instantiate `shave` client")?;

  let result = match args.command {
    Command::Screenshot(screenshot) => self::screenshot(&mut client, screenshot).await,
    Command::Launch(launch) => self::launch(&mut client, launch).await,
  };

  let () = client
    .destroy()
    .await
    .context("failed to destroy `shave` client")?;
  result
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
  run(args_os()).await
}
