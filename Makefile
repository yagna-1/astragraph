dev-up:
	docker compose up --build

dev-test:
	pytest -q

dev-dashboard:
	cd dashboard && npm install && npm run dev -- --host 0.0.0.0 --port 5173

proto-gen:
	cargo build -p astragraph-proto
	python3 -m grpc_tools.protoc -I proto --python_out=verifier/generated --grpc_python_out=verifier/generated proto/astragraph.proto

certs:
	bash scripts/generate-certs.sh
