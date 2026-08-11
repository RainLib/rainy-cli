# Enterprise Rainy Source Example

This directory is a minimal self-describing Source with one project template and one optional workspace module.

```bash
rainy source inspect examples/enterprise-source
rainy source add enterprise-example examples/enterprise-source --apply
rainy new demo-enterprise --source enterprise-example \
  --template service-base --module backend-java --apply
```

The managed Source is cached under `RAINY_HOME`; it is not copied wholesale into the current directory.
