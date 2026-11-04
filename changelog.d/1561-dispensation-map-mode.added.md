- A dispensation of a prescription a map-mode member owns is written at
  that member as one openEHR composition: the received
  `MedicationDispense-eu-mpd` is mapped by the MPD project of `eehrxf`, its
  `ACTION` names the order it carries out by the order's
  `OBJECT_VERSION_ID`, and the face creates it once with
  `composition_create`. A map-mode member named in
  `Electronic-Dispensations` names the `language` and `territory` its
  compositions are written in, and `config check` refuses it without them.
  A dispensation the owner already keeps under the same business
  identifier is refused `409` with nothing written.
