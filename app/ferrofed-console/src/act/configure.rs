// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration page's read and the post of its forms.
//!
//! [`config_now`] reads what the part of the page a path names draws from
//! the first replica, and [`configure`] reads a posted form into a
//! `ConfigForm` and runs it there (`crate::configure::act`), both as the
//! operator of the request. A form whose run changed nothing comes back as
//! posted, so the page draws it again with the field a refusal names, and
//! a secret's value never comes back.
#![cfg_attr(
    target_arch = "wasm32",
    allow(
        clippy::unused_async_trait_impl,
        reason = "the server function macro writes the browser half's call as an async trait method; only that target has it"
    )
)]

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::act::fields::Fields;
use crate::configure::draft::Draft;
#[cfg(not(target_arch = "wasm32"))]
use crate::configure::draft::Edit;
#[cfg(not(target_arch = "wasm32"))]
use crate::configure::model::ConfigForm;
use crate::configure::model::Screen;
use crate::configure::render::ConfigData;
use crate::configure::said::ActionAnswer;
use crate::read::Stop;

/// The route a post of a configuration form goes to, whose body may carry
/// an import's TOML.
pub const IMPORT: &str = "/api/configure";

/// What a post of a configuration form gave.
///
/// It holds the answer at the top of the page, and the form to draw again,
/// as posted, when it changed nothing. An Add or a Remove of a row runs
/// nothing and answers nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigAnswer {
    /// What the run gave.
    pub answer: Option<ActionAnswer>,
    /// The form to draw again.
    pub draft: Option<Draft>,
}

/// What the part of the configuration page `screen` names reads of the
/// first replica, as the operator of the request.
///
/// # Errors
/// Returns the [`Stop`] of a missing session or a console with no admin
/// listener, of a refused or failed read, and of a path that names no part
/// of the page.
#[server(endpoint = "configuration")]
pub async fn config_now(
    /// The part of the page the path names.
    #[server(default)]
    screen: Screen,
) -> Result<ConfigData, Stop> {
    let (state, token) = crate::read::signed_in()?;
    let replica = crate::configure::act::replica(&state.settings().admin)?;
    crate::configure::act::page(state.gateway(), replica, &token, &screen).await
}

/// Runs the configuration form `fields` hold at the first replica, as the
/// operator of the request.
///
/// # Errors
/// Returns the [`Stop`] of a missing session; a form that cannot be sent
/// and a replica's refusal are answers.
#[server(endpoint = "configure")]
pub async fn configure(
    /// The configuration form, every field as posted.
    #[server(flatten)]
    fields: Fields,
) -> Result<ConfigAnswer, Stop> {
    let (state, token) = crate::read::signed_in()?;
    let form = ConfigForm::of(fields.pairs());
    Ok(answered(&state, &token, form).await)
}

/// What a post of `form` gives.
#[cfg(not(target_arch = "wasm32"))]
async fn answered(
    state: &crate::server::ConsoleState,
    token: &crate::gateway::AccessToken,
    mut form: ConfigForm,
) -> ConfigAnswer {
    if let Some(edit) = Edit::of(&form.edit) {
        edit.apply(&mut form);
        return ConfigAnswer {
            answer: None,
            draft: Some(Draft {
                form: form.without_secrets(),
                field: None,
            }),
        };
    }
    let replica = match crate::configure::act::replica(&state.settings().admin) {
        Ok(replica) => replica,
        Err(refused) => {
            return ConfigAnswer {
                answer: Some(crate::configure::said::refused(&refused)),
                draft: Some(Draft {
                    form: form.without_secrets(),
                    field: None,
                }),
            };
        }
    };
    let answer = match crate::configure::plan::plan(&mut form) {
        Ok(plan) => crate::configure::act::run(state.gateway(), replica, token, plan).await,
        Err(refused) => crate::configure::render::answers::refused(&refused),
    };
    let draft = (!answer.changed).then(|| Draft {
        form: form.without_secrets(),
        field: answer.field.clone(),
    });
    ConfigAnswer {
        answer: Some(answer),
        draft,
    }
}

#[cfg(test)]
mod tests {
    use super::Configure;

    // NOTE: leptos 0.8 `ActionForm`, the browser reads the form into the function's arguments
    // with `serde_qs` and writes them again for its post, which the server reads back.
    #[test]
    fn a_form_crosses_from_the_browser_to_the_server_whole() {
        let body = "ferroui_answer=%2Fconfiguration%2Fmembers%2Fnew&form%5Baction%5D=add-member\
            &form%5Bbase%5D=2&form%5Borganisation%5D=org-c&form%5Bnode%5D=node-c\
            &form%5Bquery_rows%5D%5B%5D=0&form%5Bquery_rows%5D%5B%5D=new-1\
            &form%5Bquery.0.id%5D=node-c-pub&form%5Bquery.0.url%5D=https%3A%2F%2Fcdr.example.org\
            &form%5Bquery.new-1.id%5D=node-c-old&form%5Bquery.0.in_service%5D=on&form%5Breason%5D=add";
        let read: Configure = serde_qs::Config::new(5, false)
            .deserialize_str(body)
            .expect("the browser reads the form");
        let wire = serde_qs::to_string(&read).expect("the browser writes its post");
        let back: Configure = serde_qs::Config::new(5, false)
            .deserialize_str(&wire)
            .expect("the server reads the post");
        let mut sent = read.fields.pairs().to_vec();
        let mut got = back.fields.pairs().to_vec();
        sent.sort();
        got.sort();
        assert_eq!(sent, got);
        assert_eq!(12, got.len());
    }
}
