#!/usr/bin/env python3
"""Verify the Businex dev storage service end to end (S3 API + durability).

Run against the compose.dev.yaml MinIO service after:
  docker compose -f deploy/compose.dev.yaml up -d minio

Checks: bucket create, object put/get byte equality, head/list consistency,
and (with --restart) that a container restart keeps the artifact bytes.
Exit code 0 means every check passed.
"""
import argparse
import hashlib
import os
import pathlib
import subprocess
import sys

import boto3
from botocore.config import Config

# Loopback traffic must bypass the host HTTP(S) proxy or the S3 calls
# hit the proxy and fail with 502.
os.environ["NO_PROXY"] = "127.0.0.1,localhost"
os.environ["no_proxy"] = "127.0.0.1,localhost"

ENDPOINT = os.environ.get("BUSINEX_S3_ENDPOINT", "http://127.0.0.1:9010")
BUCKET = os.environ.get("BUSINEX_S3_BUCKET", "businex-artifacts")
KEY = "artifacts/verify-1.bin"
PAYLOAD = b"businex artifact persistence check\n" * 4096

def client():
    # Dev-stack credentials; production provisioning sets real secrets.
    key_id = os.environ.get("BUSINEX_S3_ACCESS_KEY", "businex")
    secret = os.environ.get("BUSINEX_S3_SECRET_KEY", "businex" + "_dev_only")
    return boto3.client(
        "s3",
        endpoint_url=ENDPOINT,
        aws_access_key_id=key_id,
        aws_secret_access_key=secret,
        region_name="us-east-1",
        config=Config(signature_version="s3v4", s3={"addressing_style": "path"}),
    )

def roundtrip(s3, label):
    got = s3.get_object(Bucket=BUCKET, Key=KEY)["Body"].read()
    want = hashlib.sha256(PAYLOAD).hexdigest()
    have = hashlib.sha256(got).hexdigest()
    assert have == want, f"{label}: sha256 mismatch {have} != {want}"
    head = s3.head_object(Bucket=BUCKET, Key=KEY)
    assert head["ContentLength"] == len(PAYLOAD), f"{label}: head length wrong"
    listed = sorted(o["Key"] for o in s3.list_objects_v2(Bucket=BUCKET).get("Contents", []))
    assert KEY in listed, f"{label}: object not listed: {listed}"
    print(f"{label}: sha256 {have} bytes {len(got)} list ok")

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--restart", action="store_true",
                    help="restart the minio container and re-verify durability")
    args = ap.parse_args()
    s3 = client()
    try:
        s3.create_bucket(Bucket=BUCKET)
        print("bucket created:", BUCKET)
    except Exception as exc:  # already exists is fine
        print("create_bucket note:", exc)
    s3.put_object(Bucket=BUCKET, Key=KEY, Body=PAYLOAD,
                  ContentType="application/octet-stream")
    roundtrip(s3, "upload/download")
    if args.restart:
        compose = pathlib.Path(__file__).resolve().parent.parent / "compose.dev.yaml"
        subprocess.run(
            ["docker", "compose", "-f", str(compose), "restart", "minio"],
            check=True, capture_output=True,
        )
        s3 = client()
        roundtrip(s3, "after-restart")
    print("storage verification: PASS")
    return 0

if __name__ == "__main__":
    sys.exit(main())
