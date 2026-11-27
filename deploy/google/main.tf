terraform {
  required_version = ">= 1.6, < 2.0"

  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "~> 7.0"
    }
  }
}

provider "google" {
  project = var.project_id
  region  = var.region
}

variable "project_id" {
  type = string
}

variable "region" {
  type    = string
  default = "us-west1"
}

variable "image" {
  type = string

  validation {
    condition     = can(regex("^[a-z0-9-]+-docker[.]pkg[.]dev/[^@]+@sha256:[0-9a-f]{64}$", var.image))
    error_message = "Use the published Artifact Registry image pinned by SHA-256 digest."
  }
}

variable "public_origin" {
  type = string

  validation {
    condition     = can(regex("^https://[a-z0-9-]+([.][a-z0-9-]+)+$", var.public_origin))
    error_message = "Use an HTTPS origin with a DNS hostname and no trailing slash."
  }
}

variable "media_bucket" {
  type = string
}

variable "production" {
  type    = bool
  default = true
}

variable "database_secret_version" {
  type    = string
  default = "1"

  validation {
    condition     = can(regex("^[1-9][0-9]*$", var.database_secret_version))
    error_message = "Pin the database secret to a numeric Secret Manager version."
  }
}

locals {
  secret_versions = {
    DATABASE_URL            = var.database_secret_version
    FIELD_SESSION_SECRET    = "1"
    FIELD_DIARY_KEYRING     = "1"
    FIELD_BACKUP_RECIPIENTS = "1"
  }
  web_settings = {
    FIELD_ENV           = "production"
    FIELD_STORAGE       = "gcs"
    FIELD_GCS_BUCKET    = google_storage_bucket.media.name
    FIELD_PUBLIC_ORIGIN = var.public_origin
    FIELD_RP_ID         = trimprefix(var.public_origin, "https://")
    FIELD_OUTSIDE_AI    = "off"
  }
}

resource "google_project_service" "api" {
  for_each = toset([
    "artifactregistry.googleapis.com",
    "iam.googleapis.com",
    "run.googleapis.com",
    "secretmanager.googleapis.com",
    "sqladmin.googleapis.com",
    "storage.googleapis.com",
  ])
  service            = each.key
  disable_on_destroy = false
}

resource "google_sql_database_instance" "fieldbook" {
  name                = "fieldbook-db"
  region              = var.region
  database_version    = "POSTGRES_16"
  deletion_protection = var.production

  settings {
    edition                     = "ENTERPRISE"
    tier                        = "db-g1-small"
    availability_type           = "ZONAL"
    deletion_protection_enabled = var.production

    ip_configuration {
      ipv4_enabled = true
      ssl_mode     = "ENCRYPTED_ONLY"
    }

    backup_configuration {
      enabled                        = true
      point_in_time_recovery_enabled = true
      transaction_log_retention_days = 7
    }
  }

  depends_on = [google_project_service.api]
}

resource "google_sql_database" "fieldbook" {
  name     = "fieldbook"
  instance = google_sql_database_instance.fieldbook.name
}

resource "google_storage_bucket" "media" {
  name                        = var.media_bucket
  location                    = var.region
  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"
  force_destroy               = false

  versioning {
    enabled = true
  }

  lifecycle_rule {
    action {
      type = "Delete"
    }
    condition {
      days_since_noncurrent_time = 30
      with_state                 = "ARCHIVED"
    }
  }

  depends_on = [google_project_service.api]
}

resource "google_service_account" "runtime" {
  account_id   = "fieldbook-runtime"
  display_name = "Fieldbook Cloud Run runtime"
  depends_on   = [google_project_service.api]
}

resource "google_project_iam_member" "cloud_sql" {
  project = var.project_id
  role    = "roles/cloudsql.client"
  member  = google_service_account.runtime.member
}

resource "google_storage_bucket_iam_member" "media" {
  bucket = google_storage_bucket.media.name
  role   = "roles/storage.objectUser"
  member = google_service_account.runtime.member
}

resource "google_secret_manager_secret" "runtime" {
  for_each  = local.secret_versions
  secret_id = "fieldbook-${replace(lower(each.key), "_", "-")}"

  replication {
    auto {}
  }

  depends_on = [google_project_service.api]
}

resource "google_secret_manager_secret_iam_member" "runtime" {
  for_each  = google_secret_manager_secret.runtime
  secret_id = each.value.id
  role      = "roles/secretmanager.secretAccessor"
  member    = google_service_account.runtime.member
}

resource "google_cloud_run_v2_job" "migrate" {
  name                = "fieldbook-migrate"
  location            = var.region
  deletion_protection = var.production

  template {
    template {
      service_account = google_service_account.runtime.email
      max_retries     = 0
      timeout         = "300s"

      containers {
        image   = var.image
        command = ["fieldbook"]
        args    = ["migrate"]

        env {
          name = "DATABASE_URL"
          value_source {
            secret_key_ref {
              secret  = google_secret_manager_secret.runtime["DATABASE_URL"].secret_id
              version = local.secret_versions["DATABASE_URL"]
            }
          }
        }

        volume_mounts {
          name       = "cloudsql"
          mount_path = "/cloudsql"
        }
      }

      volumes {
        name = "cloudsql"
        cloud_sql_instance {
          instances = [google_sql_database_instance.fieldbook.connection_name]
        }
      }
    }
  }

  depends_on = [
    google_project_iam_member.cloud_sql,
    google_secret_manager_secret_iam_member.runtime["DATABASE_URL"],
    google_sql_database.fieldbook,
  ]
}

resource "google_cloud_run_v2_service" "web" {
  name                 = "fieldbook-web"
  location             = var.region
  ingress              = "INGRESS_TRAFFIC_ALL"
  invoker_iam_disabled = true
  deletion_protection  = var.production
  launch_stage         = "BETA"

  scaling {
    min_instance_count = 0
    max_instance_count = 2
  }

  template {
    execution_environment = "EXECUTION_ENVIRONMENT_GEN2"
    service_account       = google_service_account.runtime.email

    containers {
      image = var.image

      ports {
        container_port = 8080
      }

      startup_probe {
        http_get {
          path = "/readyz"
          port = 8080
        }
        timeout_seconds   = 3
        period_seconds    = 5
        failure_threshold = 12
      }

      readiness_probe {
        http_get {
          path = "/readyz"
          port = 8080
        }
        timeout_seconds   = 3
        period_seconds    = 5
        failure_threshold = 3
      }

      dynamic "env" {
        for_each = local.web_settings
        content {
          name  = env.key
          value = env.value
        }
      }

      dynamic "env" {
        for_each = local.secret_versions
        content {
          name = env.key
          value_source {
            secret_key_ref {
              secret  = google_secret_manager_secret.runtime[env.key].secret_id
              version = env.value
            }
          }
        }
      }

      volume_mounts {
        name       = "cloudsql"
        mount_path = "/cloudsql"
      }
    }

    volumes {
      name = "cloudsql"
      cloud_sql_instance {
        instances = [google_sql_database_instance.fieldbook.connection_name]
      }
    }
  }

  depends_on = [
    google_cloud_run_v2_job.migrate,
    google_project_iam_member.cloud_sql,
    google_secret_manager_secret_iam_member.runtime,
    google_storage_bucket_iam_member.media,
  ]
}

output "sql_connection_name" {
  value = google_sql_database_instance.fieldbook.connection_name
}

output "service_uri" {
  value = google_cloud_run_v2_service.web.uri
}
