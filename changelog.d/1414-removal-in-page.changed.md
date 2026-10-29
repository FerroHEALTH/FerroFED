- The operator console removes a member, a credential or a binding section
  from a configuration table in the page, as every other configuration form
  does, once the page has hydrated: the confirmation dialog runs the
  removal through the configuration's server function and closes when the
  removal is saved, and stays open with the refusal otherwise. With no
  WebAssembly it still posts the form.
