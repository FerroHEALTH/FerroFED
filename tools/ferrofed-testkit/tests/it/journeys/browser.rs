// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One headless Chrome session over `WebDriver`, with the explicit waits and
//! the console check every journey uses.
//!
//! The session asks chromedriver for the browser log (`goog:loggingPrefs`),
//! and [`Browser::console_clean`] drains it and fails on any entry at the
//! `SEVERE` level: an error a script logged, an uncaught error or panic, a
//! refused Content-Security-Policy, or a resource the page could not load.
//! Every wait polls the page for an element or a state until
//! [`WAIT`] runs out, never a fixed sleep.

use std::error::Error;
use std::time::Duration;

use serde_json::json;
use thirtyfour::LoggingPrefsLogLevel;
use thirtyfour::prelude::*;

/// The environment variable naming the `WebDriver` endpoint, when it is not
/// [`WEBDRIVER`].
const WEBDRIVER_ENV: &str = "FERROFED_WEBDRIVER";

/// The endpoint a chromedriver started with its defaults listens on.
const WEBDRIVER: &str = "http://127.0.0.1:9515";

/// The environment variable naming the Chrome binary, when chromedriver is
/// not to find one itself.
const CHROME_ENV: &str = "FERROFED_CHROME";

/// How long a wait polls before it fails.
pub(crate) const WAIT: Duration = Duration::from_secs(20);

/// How often a wait polls.
const POLL: Duration = Duration::from_millis(100);

/// The log level chromedriver reports a console error, an uncaught error and
/// a failed load at.
const SEVERE: &str = "SEVERE";

/// A browser session a journey drives.
#[derive(Debug)]
pub(crate) struct Browser {
    /// The `WebDriver` session.
    driver: WebDriver,
}

impl Browser {
    /// Starts a headless Chrome session at the `WebDriver` endpoint.
    pub(crate) async fn open() -> Result<Self, Box<dyn Error>> {
        let mut capabilities = DesiredCapabilities::chrome();
        capabilities.set_headless()?;
        capabilities.set_no_sandbox()?;
        capabilities.set_disable_dev_shm_usage()?;
        capabilities.add_arg("--window-size=1280,1024")?;
        capabilities.set_browser_log_level(LoggingPrefsLogLevel::Info)?;
        if let Ok(binary) = std::env::var(CHROME_ENV) {
            capabilities.set_binary(&binary)?;
        }
        let endpoint = std::env::var(WEBDRIVER_ENV).unwrap_or_else(|_unset| WEBDRIVER.to_owned());
        let driver = WebDriver::new(endpoint.as_str(), capabilities)
            .await
            .map_err(|error| {
                format!("no WebDriver session at {endpoint} (is chromedriver running?): {error}")
            })?;
        Ok(Self { driver })
    }

    /// Ends the session and closes the browser.
    pub(crate) async fn quit(self) -> Result<(), Box<dyn Error>> {
        self.driver.quit().await?;
        Ok(())
    }

    /// The `WebDriver` session.
    pub(crate) fn driver(&self) -> &WebDriver {
        &self.driver
    }

    /// Loads `url` and waits for the document to load.
    pub(crate) async fn goto(&self, url: &str) -> Result<(), Box<dyn Error>> {
        self.driver.goto(url).await?;
        Ok(())
    }

    /// Fails on any `SEVERE` entry the browser logged since the last call,
    /// naming `step` and every entry.
    pub(crate) async fn console_clean(&self, step: &str) -> Result<(), Box<dyn Error>> {
        let entries = self.driver.browser_log().await?;
        let errors: Vec<String> = entries
            .iter()
            .filter(|entry| entry.level == SEVERE)
            .map(|entry| format!("{} {}", entry.level, entry.message))
            .collect();
        if errors.is_empty() {
            return Ok(());
        }
        Err(format!(
            "the browser logged {} console error(s) during {step}:\n{}",
            errors.len(),
            errors.join("\n")
        )
        .into())
    }

    /// Waits for the element `by` names, and returns it.
    ///
    /// A wait that runs out names the page the browser is at and its text.
    pub(crate) async fn element(&self, by: By) -> Result<WebElement, Box<dyn Error>> {
        let found = self.driver.query(by.clone()).wait(WAIT, POLL).first().await;
        match found {
            Ok(element) => Ok(element),
            Err(error) => Err(format!(
                "no element {by} at {}: {error}\nthe page reads:\n{}",
                self.driver.current_url().await?,
                self.driver.find(By::Tag("body")).await?.text().await?
            )
            .into()),
        }
    }

    /// Waits for the element `by` names, then until it is enabled.
    pub(crate) async fn enabled(&self, by: By) -> Result<(), Box<dyn Error>> {
        self.element(by)
            .await?
            .wait_until()
            .wait(WAIT, POLL)
            .enabled()
            .await?;
        Ok(())
    }

    /// Waits for an element of `tag` whose text holds `text`, and returns it.
    ///
    /// `text` must hold no apostrophe, since it is quoted in an `XPath`.
    pub(crate) async fn text(&self, tag: &str, text: &str) -> Result<WebElement, Box<dyn Error>> {
        self.element(By::XPath(format!("//{tag}[contains(., '{text}')]")))
            .await
    }

    /// Waits for the link whose text is `text`, and follows it.
    pub(crate) async fn follow(&self, text: &str) -> Result<(), Box<dyn Error>> {
        self.element(By::LinkText(text)).await?.click().await?;
        Ok(())
    }

    /// Waits until the document's title is `title`.
    pub(crate) async fn titled(&self, title: &str) -> Result<(), Box<dyn Error>> {
        self.element(By::XPath(format!("//title[. = '{title}']")))
            .await
            .map_err(|error| format!("the page is never titled {title:?}: {error}"))?;
        Ok(())
    }

    /// Waits until the browser is at `url`.
    pub(crate) async fn at(&self, url: &str) -> Result<(), Box<dyn Error>> {
        let arrived = until(&format!("the browser is at {url}"), async || {
            Ok(self.driver.current_url().await?.as_str() == url)
        })
        .await;
        match arrived {
            Ok(()) => Ok(()),
            Err(error) => {
                let entries = self.driver.browser_log().await?;
                Err(format!(
                    "{error}; it is at {}, and logged {entries:?}",
                    self.driver.current_url().await?
                )
                .into())
            }
        }
    }

    /// The cookie the browser holds for the page under `name`, if any.
    pub(crate) async fn cookie(&self, name: &str) -> Result<Option<Cookie>, Box<dyn Error>> {
        Ok(self
            .driver
            .get_all_cookies()
            .await?
            .into_iter()
            .find(|cookie| cookie.name == name))
    }

    /// Turns the page's scripts off, or back on, for every page it loads
    /// from now on (the Chrome `DevTools` Protocol,
    /// `Emulation.setScriptExecutionDisabled`).
    pub(crate) async fn scripts(&self, enabled: bool) -> Result<(), Box<dyn Error>> {
        self.driver
            .cdp()
            .send_raw(
                "Emulation.setScriptExecutionDisabled",
                json!({ "value": !enabled }),
            )
            .await?;
        Ok(())
    }
}

/// Polls `check` every [`POLL`] until it holds, and fails naming `what` when
/// [`WAIT`] runs out first.
pub(crate) async fn until<F>(what: &str, mut check: F) -> Result<(), Box<dyn Error>>
where
    F: AsyncFnMut() -> Result<bool, Box<dyn Error>>,
{
    let mut polls = 0_u32;
    let mut interval = tokio::time::interval(POLL);
    loop {
        interval.tick().await;
        if check().await? {
            return Ok(());
        }
        polls = polls.saturating_add(1);
        if POLL.saturating_mul(polls) >= WAIT {
            return Err(format!("waited {WAIT:?} and it never held that {what}").into());
        }
    }
}

/// Runs `journey` in a fresh browser, closes the browser whatever the
/// journey did, and fails on a console error the browser logged last.
pub(crate) async fn with_browser<F>(journey: F) -> Result<(), Box<dyn Error>>
where
    F: AsyncFnOnce(&Browser) -> Result<(), Box<dyn Error>>,
{
    let browser = Browser::open().await?;
    let outcome = match journey(&browser).await {
        Ok(()) => browser.console_clean("the end of the journey").await,
        Err(error) => Err(error),
    };
    browser.quit().await?;
    outcome
}
