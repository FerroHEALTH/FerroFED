- An audit record names the caller's purposes of use at `purposeOfEvent`
  too, each once, beside the `agent:user` `purposeOfUse` that BALP 1.1.4
  places them at, so an Audit Record Repository that keeps the DICOM audit
  message form keeps them, an emergency purpose such as `BTG` among them
  (RESTful ATNA Tables 3.20.4.2.2.1-1 and 3.81.4.2.2.1-1, ITI TF-2
  §3.20.7.1.2). The access log's records carry them there as well
  (`ihe-iti` 0.0.32, `ehds-logging` 0.0.14; #830).
