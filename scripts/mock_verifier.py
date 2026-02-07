#!/usr/bin/env python3
"""Mock verifier gRPC service with mTLS for dockerized E2E tests."""

from __future__ import annotations

from concurrent import futures
import os
import sys
import time

import grpc

GEN_DIR = os.path.join(os.path.dirname(__file__), "..", "verifier", "generated")
GEN_DIR = os.path.abspath(GEN_DIR)
if GEN_DIR not in sys.path:
    sys.path.insert(0, GEN_DIR)

import astragraph_pb2  # type: ignore
import astragraph_pb2_grpc  # type: ignore


class MockVerifierService(astragraph_pb2_grpc.VerifierServiceServicer):
    def ScoreAction(self, request, context):  # noqa: N802
        start = time.perf_counter()
        deviation = 0.95 if "export_data" in request.agent_action else 0.05
        latency_ms = int((time.perf_counter() - start) * 1000)
        return astragraph_pb2.VerifierResponse(
            deviation_score=deviation,
            verifier_model="mock-verifier",
            latency_ms=latency_ms,
            verifier_thinking="mock-verifier: deterministic score",
        )

    def StreamScore(self, request_iterator, context):  # noqa: N802
        for request in request_iterator:
            yield self.ScoreAction(request, context)


def main() -> int:
    grpc_addr = os.getenv("ASTRAGRAPH_VERIFIER_GRPC_ADDR", "0.0.0.0:8082")
    tls_cert = os.getenv("ASTRAGRAPH_VERIFIER_TLS_CERT", "")
    tls_key = os.getenv("ASTRAGRAPH_VERIFIER_TLS_KEY", "")
    tls_ca = os.getenv("ASTRAGRAPH_VERIFIER_TLS_CA", "")
    if not (tls_cert and tls_key and tls_ca):
        raise RuntimeError("mTLS cert/key/ca env vars are required for mock verifier")

    server = grpc.server(futures.ThreadPoolExecutor(max_workers=4))
    astragraph_pb2_grpc.add_VerifierServiceServicer_to_server(MockVerifierService(), server)

    with open(tls_cert, "rb") as cert_file, open(tls_key, "rb") as key_file, open(
        tls_ca, "rb"
    ) as ca_file:
        credentials = grpc.ssl_server_credentials(
            [(key_file.read(), cert_file.read())],
            root_certificates=ca_file.read(),
            require_client_auth=True,
        )
    server.add_secure_port(grpc_addr, credentials)
    server.start()
    server.wait_for_termination()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
