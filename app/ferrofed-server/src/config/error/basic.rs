// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What RFC 7617 §2 forbids in a basic credential the configuration names.

use std::fmt;

/// What RFC 7617 §2 forbids in a basic user-id or password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BasicFault {
    /// A control character (`CTL` of RFC 5234 Appendix B.1), forbidden in
    /// both the user-id and the password.
    ControlCharacter,
    /// A colon, forbidden in the user-id.
    Colon,
}

impl fmt::Display for BasicFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ControlCharacter => f.write_str("a control character"),
            Self::Colon => f.write_str("a colon"),
        }
    }
}
