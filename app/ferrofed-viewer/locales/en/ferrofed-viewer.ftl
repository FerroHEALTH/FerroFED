# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1

# The English catalog of the console's own words: the reference catalog and
# the one every lookup falls back to. Each message is one `Msg` of the
# crate's `locale` module; a catalog of another language holds the same
# messages in a directory named by its BCP 47 tag beside this one. The words
# the FerroUI components draw themselves are that library's catalog.

## The page for a session store the console cannot read

unreadable-title = Fault
unreadable-heading = The console cannot read its sessions
unreadable-fault = 500: the console cannot read its session store, so it cannot tell whether you are signed in. Ask the console's operator to restart it.
unreadable-retry = Try again

## The page for a path the console does not serve

not-found-title = Not found
not-found-body = 404: the console has no page at this address.
not-found-back = Back to the console
