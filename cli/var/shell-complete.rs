// Copyright (C) 2024-2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::io::stdout;

use clap::CommandFactory as _;
use clap::Parser;

use clap_complete::generate;
use clap_complete::Shell;


#[allow(unused)]
mod prog {
  include!("../src/args.rs");
}


/// Generate a shell completion script for the program.
#[derive(Debug, Parser)]
struct Args {
  /// The shell for which to generate a completion script for.
  #[clap(value_enum)]
  shell: Shell,
  /// The command for which to generate the shell completion script.
  #[clap(default_value = env!("CARGO_PKG_NAME"))]
  command: String,
}


fn main() {
  let args = Args::parse();
  let mut app = prog::Args::command();
  generate(args.shell, &mut app, &args.command, &mut stdout());
}
