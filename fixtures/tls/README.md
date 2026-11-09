# Test TLS fixtures

**Test-only. Never use these in production.** The private key is public.

- `ca.pem`: a self-signed P-256 test CA. Clients trust this file.
- `cert.pem`: a leaf for `localhost` and `127.0.0.1`, signed by the CA.
- `key.pem`: the leaf's PKCS#8 private key.

All three are valid for 100 years. The CA key was discarded after signing. To regenerate, create a new CA and leaf with `openssl req`/`openssl x509`, using the same extensions: CA `basicConstraints=CA:TRUE`; leaf `CA:FALSE`, `serverAuth`, and SAN `DNS:localhost,IP:127.0.0.1`.
