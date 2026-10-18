terraform {
  required_version = ">= 1.5.0"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }
}

variable "project" {
  type        = string
  description = "Project / name prefix"
  default     = "stellarlend"

  validation {
    condition     = can(regex("^[a-z0-9-]{1,30}$", var.project))
    error_message = "project must contain 1-30 lowercase letters, digits, or hyphens."
  }
}

variable "environment" {
  type        = string
  description = "Deployment environment used in resource names."
  default     = "dev"

  validation {
    condition     = can(regex("^[a-z0-9-]{1,12}$", var.environment))
    error_message = "environment must contain 1-12 lowercase letters, digits, or hyphens."
  }
}

variable "aws_region" {
  type        = string
  description = "AWS region for the data lake resources."
  default     = "us-east-1"
}

variable "raw_retention_days" {
  type        = number
  description = "Number of days to retain raw lake objects."
  default     = 90

  validation {
    condition     = var.raw_retention_days >= 1 && floor(var.raw_retention_days) == var.raw_retention_days
    error_message = "raw_retention_days must be a positive whole number of days."
  }
}

variable "bucket_name" {
  type        = string
  description = "Optional globally-unique S3 bucket name; defaults to project-environment-raw-blockchain."
  default     = ""

  validation {
    condition     = var.bucket_name == "" || can(regex("^[a-z0-9][a-z0-9.-]{1,61}[a-z0-9]$", var.bucket_name))
    error_message = "bucket_name must be a valid 3-63 character S3 bucket name."
  }
}

variable "force_destroy" {
  type        = bool
  description = "Allow Terraform to delete a non-empty lake bucket. Keep false for durable environments."
  default     = false
}

provider "aws" {
  region = var.aws_region
}

data "aws_caller_identity" "current" {}

locals {
  bucket_name = var.bucket_name != "" ? var.bucket_name : "${var.project}-${var.environment}-raw-blockchain"
}

resource "aws_s3_bucket" "raw_lake" {
  bucket        = local.bucket_name
  force_destroy = var.force_destroy
}

resource "aws_s3_bucket_ownership_controls" "raw_lake" {
  bucket = aws_s3_bucket.raw_lake.id

  rule {
    object_ownership = "BucketOwnerEnforced"
  }
}

resource "aws_s3_bucket_server_side_encryption_configuration" "raw_lake" {
  bucket = aws_s3_bucket.raw_lake.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}

resource "aws_s3_bucket_policy" "raw_lake_tls" {
  bucket = aws_s3_bucket.raw_lake.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Sid       = "DenyInsecureTransport"
      Effect    = "Deny"
      Principal = "*"
      Action    = "s3:*"
      Resource = [
        aws_s3_bucket.raw_lake.arn,
        "${aws_s3_bucket.raw_lake.arn}/*"
      ]
      Condition = {
        Bool = {
          "aws:SecureTransport" = "false"
        }
      }
    }]
  })
}

resource "aws_s3_bucket_versioning" "raw_lake" {
  bucket = aws_s3_bucket.raw_lake.id
  versioning_configuration {
    status = "Enabled"
  }
}

resource "aws_s3_bucket_lifecycle_configuration" "raw_retention" {
  bucket = aws_s3_bucket.raw_lake.id

  rule {
    id     = "expire-raw-after-retention"
    status = "Enabled"

    filter {
      prefix = "raw/"
    }

    expiration {
      days = var.raw_retention_days
    }

    noncurrent_version_expiration {
      noncurrent_days = var.raw_retention_days
    }

    abort_incomplete_multipart_upload {
      days_after_initiation = 7
    }
  }

  rule {
    id     = "keep-aggregates-indefinitely"
    status = "Enabled"

    filter {
      prefix = "agg/"
    }

    transition {
      days          = 30
      storage_class = "STANDARD_IA"
    }
  }
}

resource "aws_s3_bucket_public_access_block" "raw_lake" {
  bucket                  = aws_s3_bucket.raw_lake.id
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_glue_catalog_database" "lake" {
  name = "${var.project}_${var.environment}_lake"
}

resource "aws_glue_catalog_table" "raw_transactions" {
  name          = "raw_transactions"
  database_name = aws_glue_catalog_database.lake.name
  table_type    = "EXTERNAL_TABLE"

  parameters = {
    classification = "parquet"
    EXTERNAL       = "TRUE"
  }

  storage_descriptor {
    location      = "s3://${aws_s3_bucket.raw_lake.bucket}/raw/"
    input_format  = "org.apache.hadoop.hive.ql.io.parquet.MapredParquetInputFormat"
    output_format = "org.apache.hadoop.hive.ql.io.parquet.MapredParquetOutputFormat"

    ser_de_info {
      name                  = "parquet"
      serialization_library = "org.apache.hadoop.hive.ql.io.parquet.serde.ParquetHiveSerDe"
    }

    columns {
      name = "ledger"
      type = "bigint"
    }
    columns {
      name = "tx_hash"
      type = "string"
    }
    columns {
      name = "contract_id"
      type = "string"
    }
    columns {
      name = "event_type"
      type = "string"
    }
    columns {
      name = "event_index"
      type = "int"
    }
    columns {
      name = "block_timestamp"
      type = "bigint"
    }
    columns {
      name = "user_address"
      type = "string"
    }
    columns {
      name = "asset_address"
      type = "string"
    }
    columns {
      name = "amount"
      type = "string"
    }
    columns {
      name = "payload_json"
      type = "string"
    }
    columns {
      name = "schema_version"
      type = "int"
    }
  }

  partition_keys {
    name = "date"
    type = "string"
  }

  partition_keys {
    name = "event_type"
    type = "string"
  }
}

resource "aws_iam_role" "lake_reader" {
  name = "${var.project}-${var.environment}-lake-reader"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect = "Allow"
      Principal = {
        Service = "athena.amazonaws.com"
      }
      Action = "sts:AssumeRole"
    }]
  })
}

resource "aws_iam_role_policy" "lake_reader" {
  name = "${var.project}-${var.environment}-lake-reader"
  role = aws_iam_role.lake_reader.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect   = "Allow"
        Action   = ["s3:GetObject"]
        Resource = ["${aws_s3_bucket.raw_lake.arn}/raw/*", "${aws_s3_bucket.raw_lake.arn}/agg/*"]
      },
      {
        Effect   = "Allow"
        Action   = ["s3:ListBucket"]
        Resource = [aws_s3_bucket.raw_lake.arn]
        Condition = {
          StringLike = {
            "s3:prefix" = ["raw", "raw/*", "agg", "agg/*"]
          }
        }
      },
      {
        Effect = "Allow"
        Action = ["glue:GetDatabase"]
        Resource = [
          "arn:aws:glue:${var.aws_region}:${data.aws_caller_identity.current.account_id}:catalog",
          aws_glue_catalog_database.lake.arn
        ]
      },
      {
        Effect   = "Allow"
        Action   = ["glue:GetTable", "glue:GetPartitions"]
        Resource = [aws_glue_catalog_table.raw_transactions.arn]
      }
    ]
  })
}

resource "aws_iam_role" "lake_writer" {
  name = "${var.project}-${var.environment}-lake-writer"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect = "Allow"
      Principal = {
        Service = "ecs-tasks.amazonaws.com"
      }
      Action = "sts:AssumeRole"
    }]
  })
}

resource "aws_iam_role_policy" "lake_writer" {
  name = "${var.project}-${var.environment}-lake-writer"
  role = aws_iam_role.lake_writer.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect = "Allow"
        Action   = ["s3:PutObject", "s3:AbortMultipartUpload"]
        Resource = ["${aws_s3_bucket.raw_lake.arn}/raw/*", "${aws_s3_bucket.raw_lake.arn}/agg/*"]
      },
      {
        Effect   = "Allow"
        Action   = ["s3:ListBucket"]
        Resource = [aws_s3_bucket.raw_lake.arn]
        Condition = {
          StringLike = {
            "s3:prefix" = ["raw", "raw/*", "agg", "agg/*"]
          }
        }
      }
    ]
  })
}

output "bucket_name" {
  value       = aws_s3_bucket.raw_lake.bucket
  description = "S3 bucket for raw and aggregated lake data."
}

output "glue_database" {
  value       = aws_glue_catalog_database.lake.name
  description = "Glue database containing the lake tables."
}

output "reader_role_arn" {
  value       = aws_iam_role.lake_reader.arn
  description = "IAM role ARN for read-only lake access."
}

output "writer_role_arn" {
  value       = aws_iam_role.lake_writer.arn
  description = "IAM role ARN for lake ETL writes."
}
