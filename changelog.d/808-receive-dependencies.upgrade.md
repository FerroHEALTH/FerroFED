- Each `[[fhir.receive]]` names, in `dependencies`, every package of its
  `package`'s dependency closure (each a `.tgz` or an unpacked directory),
  and the package of the MyHealth@EU value sets its profiles bind. A
  `[[fhir.receive]]` whose packages lack one a manifest names is refused at
  start and by `config check`, naming the missing package. For the HL7
  Europe Patient Summary, the book's receiving page lists the packages
  (#808).
