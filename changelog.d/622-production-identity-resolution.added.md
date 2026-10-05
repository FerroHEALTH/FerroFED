- The identity page of the book says how each member's `ehr_id`s reach the
  PIX Manager: the PIXm ITI-104 feed or the PMIR ITI-93 feed from each
  member, the bulk load of the EHRs that already exist, why the gateway
  feeds no Manager itself, and what to do when the hospital MPI does not
  answer PIXm (#622). It names SanteMPI 2.5.12 as a PIX Manager verified
  against two CDRs, with its setup and its limits, and the products checked
  that do not answer as the gateway needs.
- The end-to-end lane resolves a patient through SanteMPI, a deployable PIX
  Manager each member feeds over ITI-93, and gets a federated query answered
  by both members (#622).
