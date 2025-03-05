0.2.4
-----
- Fixed leakage of temporary Chrome data directory
- Adjusted `chromedriver` flags


0.2.3
-----
- Switched over to `chromedriver-launch` crate for Chromedriver process
  management
- Adjusted `chromedriver` flags


0.2.2
-----
- Added `remove_selector` and `window_size` attributes to `ScreenshotOpts`


0.2.1
-----
- Updated `fantoccini` dependency to `0.21`


0.2.0
-----
- Introduced `Client` type providing previously freestanding
  `screenshot` functionality
  - Added `Builder` type for configurable `Client` creation
  - Added `Builder::set_user_agent` method for configuration of user
    agent to use


0.1.0
-----
- Initial release
