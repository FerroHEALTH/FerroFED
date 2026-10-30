// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration page's read and the post of its forms.
//!
//! [`config_now`] reads what the part of the page a path names draws from
//! the first replica, and [`configure`] reads a posted form into a
//! `ConfigForm` and runs it there (`crate::configure::act`), both as the
//! operator of the request. The form posts to the configuration page it is
//! on, which the server draws again with the answer. A form whose run changed nothing comes back as
//! posted, so the page draws it again with the field a refusal names, and
//! a secret's value never comes back.

use crate::act::fields::Fields;
use crate::configure::draft::Draft;
use crate::configure::draft::Edit;
use crate::configure::model::ConfigForm;
use crate::configure::model::Screen;
use crate::configure::render::ConfigData;
use crate::configure::said::ActionAnswer;
use crate::read::Stop;

/// The page whose form carries an import's TOML, which takes the larger
/// body limit.
pub const IMPORT: &str = "/configuration/import-export";

/// What a post of a configuration form gave.
///
/// It holds the answer at the top of the page, and the form to draw again,
/// as posted, when it changed nothing. An Add or a Remove of a row runs
/// nothing and answers nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
pub async fn config_now(screen: Screen) -> Result<ConfigData, Stop> {
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
pub async fn configure(fields: &Fields) -> Result<ConfigAnswer, Stop> {
    let (state, token) = crate::read::signed_in()?;
    let form = ConfigForm::of(fields.pairs());
    Ok(answered(&state, &token, form).await)
}

/// What a post of `form` gives.
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
