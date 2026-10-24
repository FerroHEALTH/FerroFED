# TM_resources - Transformation Manager Resources

This folder contains resources used by the OpenNCP Transformation Manager at runtime.
Files are resolved relative to `$EPSOS_PROPS_PATH` (set to `/opt/openncp-configuration/` in Docker).

## Coded Element List (`coded_element_list_ehdsi.xml`)

The default eHDSI coded element list is **bundled in the `openncp-core-transformation` JAR** on the classpath
(source: `openncp-core/openncp-core-transformation/src/main/resources/coded_element_list_ehdsi.xml`).

It is loaded automatically when `tm.codedelementlist.overridden=false` (the default).

### Overriding with a country-specific file

To use a custom coded element list:

1. Place your customized `coded_element_list_ehdsi.xml` in this folder (`TM_resources/`).
2. Set the following in `tm.properties`:
   ```properties
   tm.codedelementlist.overridden=true
   tm.codedelementlist.path=TM_resources/coded_element_list_ehdsi.xml
   ```

The application will then load the file from `$EPSOS_PROPS_PATH/TM_resources/coded_element_list_ehdsi.xml`
instead of the default bundled in the JAR.

## Other resources

| Resource | Description |
|----------|-------------|
| `schema/` | CDA schema files (XSD) used for document validation |
| `schematron/` | Schematron rules for Patient Summary, ePrescription, and eDispensation validation |
| `xsl/` | XSL stylesheets used by the schematron validation pipeline |