// Copyright (C) 2024-2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;
use std::str::FromStr;

use anyhow::ensure;
use anyhow::Context as _;
use anyhow::Error;
use anyhow::Result;

use clap::Args as Arguments;
use clap::Parser;
use clap::Subcommand;


#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Output {
  /// Save the screenshot to the file identified by the given path.
  Path(PathBuf),
  /// Write the PNG screenshot data to standard output.
  Stdout,
}

impl FromStr for Output {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "-" => Ok(Output::Stdout),
      _ => Ok(Output::Path(PathBuf::from(s))),
    }
  }
}


/// Parse a window size specification from a string.
fn parse_window_size(s: &str) -> Result<(usize, usize)> {
  let mut it = s.split(&['x', ',', ' ']);
  let w_str = it
    .next()
    .context("failed to find width coordinate in provided window size")?;
  let h_str = it
    .next()
    .context("failed to find height coordinate in provided window size")?;

  ensure!(
    it.next().is_none(),
    "unable to parse window size; encountered trailing input"
  );

  let w = usize::from_str(w_str)
    .with_context(|| format!("failed to parse width string `{w_str}` as number"))?;
  let h = usize::from_str(h_str)
    .with_context(|| format!("failed to parse height string `{w_str}` as number"))?;
  Ok((w, h))
}


/// A program for shaving data from a URL.
#[derive(Debug, Parser)]
#[clap(version = env!("VERSION"))]
pub(crate) struct Args {
  #[command(subcommand)]
  pub command: Command,
  /// Set the user agent to use.
  #[clap(long, global = true)]
  pub user_agent: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
  /// Capture a screenshot of the rendered page (or part of it).
  Screenshot(Screenshot),
  /// Launch the browser in non-headless mode and wait for user input
  /// before shutting it down again.
  ///
  /// This command is mostly meant for debugging purposes.
  Launch(Launch),
}

/// A type representing the `screenshot` command.
#[derive(Debug, Arguments)]
pub(crate) struct Screenshot {
  /// The URL to navigate to.
  pub url: String,
  /// The dimensions (WxH) of the window to configure, in pixels.
  #[clap(short, long, value_parser = parse_window_size)]
  pub window_size: Option<(usize, usize)>,
  /// The CSS selector describing an element to wait for before
  /// capturing a screenshot.
  #[clap(short, long)]
  pub await_selector: Option<String>,
  /// The selector identifying one or more elements to remove before the
  /// screenshot is captured.
  #[clap(short, long)]
  pub remove_selector: Option<String>,
  /// The selector describing the element to screenshot.
  #[clap(short, long)]
  pub selector: Option<String>,
  /// The path to the file to write the screenshot to.
  ///
  /// If not present, write to `./<screenshot-{date}.png>` in the
  /// current directory. Set to `-` to print data to standard output.
  #[clap(short, long)]
  pub output: Option<Output>,
}

/// A type representing the `launch` command.
#[derive(Debug, Arguments)]
pub(crate) struct Launch {}


#[cfg(test)]
mod tests {
  use super::*;


  /// Check that we can parse a window size specification.
  #[test]
  fn window_size_parsing() {
    assert_eq!(parse_window_size("1,2").unwrap(), (1, 2));
    assert_eq!(parse_window_size("3000x2000").unwrap(), (3000, 2000));
    assert_eq!(parse_window_size("3840 2160").unwrap(), (3840, 2160));
  }

  /// Check that we can parse an [`Output`] object from a string.
  #[test]
  fn output_parsing() {
    assert_eq!(Output::from_str("-").unwrap(), Output::Stdout);
    assert_eq!(
      Output::from_str("/tmp/path").unwrap(),
      Output::Path(PathBuf::from("/tmp/path"))
    );
  }
}
