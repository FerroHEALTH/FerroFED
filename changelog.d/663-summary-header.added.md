- The patient summary's `Patient` carries the header the demographics
  binding holds (eHN PS A.1.1, A.1.2, #663): the names, birth date,
  administrative gender, addresses and phone numbers and email addresses the
  PDQm Supplier of `[pdqm]` gives for the identifier the summary was asked
  by, asked before any member and never sent to one (N33). An element the
  Supplier does not hold is left out, and a missing birth date carries a
  `data-absent-reason` of `unknown`. A patient the Supplier does not know is
  a `404`, several patients under one identifier a `422`
  `multiple-matches`, a patient with no name a `422` `required`, and a
  Supplier that does not answer a `502` or `504`; none of them asks a member.
