# StellarLend Data Lake

Implements [issue #456](https://github.com/Smartdevs17/stellarlend/issues/456).

## Features

- Raw transaction/event data stored in **Parquet** under Hive partitions `date=` / `event_type=`
- Daily ETL pipeline (`etl/` + `npm run etl:daily`)
- Additive **schema evolution** helpers for contract upgrades
- Athena/Glue/Presto-compatible catalog (`src/catalog`, Terraform Glue table)
- Raw retention **90 days**; aggregates under `agg/` kept indefinitely
- IAM reader/writer roles via Terraform

## Layout

```
services/data-lake/
  etl/                 # issue-scoped ETL exports
  schemas/             # Parquet / Glue schemas
  infra/terraform/     # S3 + Glue + IAM
  docs/querying.md     # data scientist query patterns
  src/
```

## Local run

```bash
cd services/data-lake
cp .env.example .env
npm install
npm test
npm run etl:daily -- 2026-07-25
```

## Provision AWS infrastructure

Terraform resources are defined in `infra/terraform/`. Terraform 1.5+ and
AWS provider 5.x are required. Configure AWS credentials for the deployment
account, then review and apply the plan:

```bash
cd infra/terraform
terraform init
terraform fmt -check
terraform validate
terraform plan -out=tfplan
terraform show tfplan
terraform apply tfplan
```

Set `project`, `environment`, and `aws_region` through a `*.tfvars` file or
`-var` arguments. S3 bucket names are globally unique; set `bucket_name` if
the default `${project}-${environment}-raw-blockchain` is already taken.
The bucket is versioned, encrypted, private, and protected from non-TLS
requests. `force_destroy` defaults to `false`; do not enable it for durable
environments. Terraform state contains resource identifiers and should be
stored in a secured, access-controlled backend. The backend must be bootstrapped
separately before migrating state with `terraform init -migrate-state`.
