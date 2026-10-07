# MinIO - third-party component notice

This directory builds the MinIO object storage server used as the
S3-compatible storage service of the Businex development and self-hosting
stacks.

- Upstream project: MinIO (https://github.com/minio/minio)
- Version: RELEASE.2025-10-15T17-29-55Z
- Source commit: 9e49d5e7a648f00e26f2246f4dc28e6b07f8c84a
- License: GNU Affero General Public License v3.0 or later (AGPL-3.0-or-later)
- License text: copied into the image at /usr/share/licenses/minio/LICENSE

MinIO is an independent program distributed under its own license. It is
built from unmodified official source and run as a separate service;
the Businex codebase itself remains licensed Apache-2.0. Modifications to
MinIO itself would be subject to the AGPL source obligations of that
license. As of this notice no local modifications are made to the upstream
source: the Dockerfile only compiles it with upstream build flags.

The image is built from source (not pulled from a registry) because the
published registry images were not retrievable in this environment; the
pinned tag and commit above make the build reproducible and auditable.
