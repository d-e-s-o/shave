// Copyright (C) 2024-2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

//! A library for ~~scraping~~ shaving data from websites.

mod client;

pub use client::Builder;
pub use client::Client;
pub use client::ScreenshotOpts;
