"""vLLM-based verification service with gRPC."""

from dataclasses import dataclass
from concurrent import futures
import os
import sys
import time
from typing import Optional

import grpc
from vllm import LLM, SamplingParams

from scoring import build_prompt, parse_score

GEN_DIR = os.path.join(os.path.dirname(__file__), "generated")
if GEN_DIR not in sys.path:
    sys.path.insert(0, GEN_DIR)

try:
    import astragraph_pb2
    import astragraph_pb2_grpc
except ImportError as exc:  # pragma: no cover - requires proto-gen
    raise RuntimeError("Run `make proto-gen` to generate gRPC stubs") from exc


@dataclass
class VerifierConfig:
    model: str = "LiquidAI/LFM2.5-1.2B-Thinking"
    quantization: str = "bitsandbytes"
    max_model_len: int = 2048
    gpu_memory_utilization: float = 0.85
    temperature: float = 0.0
    max_tokens: int = 512
    stop: tuple[str, ...] = ("</think>",)
    grpc_addr: str = "0.0.0.0:8080"
    tls_cert: str = os.getenv("ASTRAGRAPH_VERIFIER_TLS_CERT", "")
    tls_key: str = os.getenv("ASTRAGRAPH_VERIFIER_TLS_KEY", "")
    tls_ca: str = os.getenv("ASTRAGRAPH_VERIFIER_TLS_CA", "")

    @classmethod
    def from_env(cls) -> "VerifierConfig":
        return cls(
            model=os.getenv("ASTRAGRAPH_VERIFIER_MODEL", cls.model),
            quantization=os.getenv("ASTRAGRAPH_VERIFIER_QUANTIZATION", cls.quantization),
            max_model_len=int(os.getenv("ASTRAGRAPH_VERIFIER_MAX_MODEL_LEN", cls.max_model_len)),
            gpu_memory_utilization=float(
                os.getenv(
                    "ASTRAGRAPH_VERIFIER_GPU_MEMORY_UTILIZATION",
                    cls.gpu_memory_utilization,
                )
            ),
            temperature=float(os.getenv("ASTRAGRAPH_VERIFIER_TEMPERATURE", cls.temperature)),
            max_tokens=int(os.getenv("ASTRAGRAPH_VERIFIER_MAX_TOKENS", cls.max_tokens)),
            grpc_addr=os.getenv("ASTRAGRAPH_VERIFIER_GRPC_ADDR", cls.grpc_addr),
            tls_cert=os.getenv("ASTRAGRAPH_VERIFIER_TLS_CERT", ""),
            tls_key=os.getenv("ASTRAGRAPH_VERIFIER_TLS_KEY", ""),
            tls_ca=os.getenv("ASTRAGRAPH_VERIFIER_TLS_CA", ""),
        )


class VerifierService:
    def __init__(self, config: VerifierConfig) -> None:
        self.config = config
        self.llm = self._build_model()
        self.sampling_params = self._build_sampling_params()

    def _build_model(self) -> LLM:
        return LLM(
            model=self.config.model,
            quantization=self.config.quantization,
            max_model_len=self.config.max_model_len,
            gpu_memory_utilization=self.config.gpu_memory_utilization,
            trust_remote_code=True,
        )

    def _build_sampling_params(self) -> SamplingParams:
        return SamplingParams(
            temperature=self.config.temperature,
            max_tokens=self.config.max_tokens,
            stop=list(self.config.stop),
        )

    def score(self, policy: str, reasoning: str, action: str) -> tuple[float, str]:
        prompt = build_prompt(policy, reasoning, action)
        with _start_span("astragraph.verifier.score"):
            outputs = self.llm.generate(
                [prompt],
                sampling_params=self.sampling_params,
                use_tqdm=False,
            )
            if not outputs:
                return 1.0, ""
            text = outputs[0].outputs[0].text
            return parse_score(text), text


class VerifierGrpc(astragraph_pb2_grpc.VerifierServiceServicer):
    def __init__(self, service: VerifierService) -> None:
        self.service = service

    def ScoreAction(self, request, context):
        start = time.perf_counter()
        score, thinking = self.service.score(
            request.policy_text, request.agent_reasoning, request.agent_action
        )
        latency_ms = int((time.perf_counter() - start) * 1000)
        return astragraph_pb2.VerifierResponse(
            deviation_score=score,
            verifier_model=self.service.config.model,
            latency_ms=latency_ms,
            verifier_thinking=thinking,
        )

    def StreamScore(self, request_iterator, context):
        for request in request_iterator:
            yield self.ScoreAction(request, context)


def serve(config: Optional[VerifierConfig] = None) -> VerifierService:
    _init_telemetry()
    cfg = config or VerifierConfig.from_env()
    service = VerifierService(cfg)

    server = grpc.server(futures.ThreadPoolExecutor(max_workers=4))
    astragraph_pb2_grpc.add_VerifierServiceServicer_to_server(VerifierGrpc(service), server)

    if not (cfg.tls_cert and cfg.tls_key and cfg.tls_ca):
        raise RuntimeError("mTLS is required for verifier gRPC")

    with open(cfg.tls_cert, "rb") as cert_file, open(cfg.tls_key, "rb") as key_file, open(
        cfg.tls_ca, "rb"
    ) as ca_file:
        credentials = grpc.ssl_server_credentials(
            [(key_file.read(), cert_file.read())],
            root_certificates=ca_file.read(),
            require_client_auth=True,
        )
    server.add_secure_port(cfg.grpc_addr, credentials)

    server.start()
    server.wait_for_termination()
    return service


if __name__ == "__main__":
    serve()


def _init_telemetry() -> None:
    try:
        from opentelemetry import trace
        from opentelemetry.sdk.resources import Resource
        from opentelemetry.sdk.trace import TracerProvider

        provider = TracerProvider(resource=Resource.create({"service.name": "astragraph-verifier"}))
        trace.set_tracer_provider(provider)
    except Exception:
        return


class _SpanContextManager:
    def __init__(self, name: str) -> None:
        self.name = name
        self._span = None

    def __enter__(self):
        try:
            from opentelemetry import trace

            tracer = trace.get_tracer(__name__)
            self._span = tracer.start_span(self.name)
            self._span.__enter__()
        except Exception:
            self._span = None
        return self

    def __exit__(self, exc_type, exc, tb):
        if self._span:
            self._span.__exit__(exc_type, exc, tb)


def _start_span(name: str) -> _SpanContextManager:
    return _SpanContextManager(name)
