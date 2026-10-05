---
name: subagent-reports-to-file
description: "A subagent's long report is written to a scratchpad file by the agent itself, because a truncated result is lost and a finished agent cannot be resumed; carried from FerroBRIDGE (2026-09-12)"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On FerroBRIDGE on 2026-09-12 two long research reports came back truncated,
and messaging the completed agents failed with "No transcript found for agent
ID", so half of each report had to be recovered from the transcript by
script.

**Why:** the orchestrator sees only the final message, cut at a size limit,
and a finished agent is not resumable.

**How to apply:** every research or register prompt tells the agent to Write
its full report to a named file under the session scratchpad and to reply
with the path and line count; the orchestrator reads the file.
