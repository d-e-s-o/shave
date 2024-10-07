// Copyright (C) 2024 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;
use std::str::FromStr;

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
}

/// A type representing the `screenshot` command.
#[derive(Debug, Arguments)]
pub(crate) struct Screenshot {
  /// The URL to navigate to.
  pub url: String,
  /// The CSS selector describing an element to wait for before
  /// capturing a screenshot.
  #[clap(short, long)]
  pub await_selector: Option<String>,
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


#[cfg(test)]
mod tests {
  use super::*;


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
