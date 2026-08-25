# Voting dApp static build for Coolify. Build context: repository root.
#   docker build -f deploy/docker/frontend.Dockerfile \
#     --build-arg VITE_OPERATOR_VOTING_URL=https://voting-api.example \
#     --build-arg VITE_WC_PROJECT_ID=... \
#     -t voting-frontend .
#
# Invariants: never pass VITE_PLAYWRIGHT_E2E, VITE_DEV_MNEMONIC, or any
# VITE_* BSC JSON-RPC URL. vite.config.ts also fails the production build
# if the Legal hatch is enabled. See docs/OPS.md and docs/FRONTEND.md.

FROM node:22-bookworm-slim AS builder

WORKDIR /app
COPY frontend/package.json frontend/package-lock.json frontend/.npmrc ./
ARG GITLAB_NPM_TOKEN=
RUN if [ -n "$GITLAB_NPM_TOKEN" ]; then \
      echo "//gitlab.com/api/v4/projects/82547916/packages/npm/:_authToken=${GITLAB_NPM_TOKEN}" >> .npmrc; \
    fi \
  && npm ci

COPY frontend/ ./

ARG VITE_OPERATOR_VOTING_URL
ARG VITE_CL8Y_TOKEN_ADDRESS=terra16wtml2q66g82fdkx66tap0qjkahqwp4lwq3ngtygacg5q0kzycgqvhpax3
ARG VITE_LEGAL_PROPERTY=vote.cl8y.com
ARG VITE_LEGAL_API_BASE_URL=https://api.terms.cl8y.com
ARG VITE_LEGAL_TERMS_BASE_URL=https://terms.cl8y.com
ARG VITE_WC_PROJECT_ID=
ARG VITE_NETWORK=mainnet

# Fail closed: these must never be present on a Coolify / production image.
ARG VITE_PLAYWRIGHT_E2E=
ARG VITE_DEV_MNEMONIC=
ARG VITE_BSC_RPC=
ARG VITE_BSC_RPC_URL=

ENV VITE_OPERATOR_VOTING_URL=$VITE_OPERATOR_VOTING_URL \
    VITE_CL8Y_TOKEN_ADDRESS=$VITE_CL8Y_TOKEN_ADDRESS \
    VITE_LEGAL_PROPERTY=$VITE_LEGAL_PROPERTY \
    VITE_LEGAL_API_BASE_URL=$VITE_LEGAL_API_BASE_URL \
    VITE_LEGAL_TERMS_BASE_URL=$VITE_LEGAL_TERMS_BASE_URL \
    VITE_WC_PROJECT_ID=$VITE_WC_PROJECT_ID \
    VITE_NETWORK=$VITE_NETWORK \
    NODE_OPTIONS=--max-old-space-size=4096

RUN test -n "$VITE_OPERATOR_VOTING_URL" \
  && test -z "$VITE_PLAYWRIGHT_E2E" \
  && test -z "$VITE_DEV_MNEMONIC" \
  && test -z "$VITE_BSC_RPC" \
  && test -z "$VITE_BSC_RPC_URL" \
  && case "$VITE_OPERATOR_VOTING_URL" in https://*) ;; *) echo "VITE_OPERATOR_VOTING_URL must be https://" >&2; exit 1 ;; esac \
  && npm run build

FROM nginx:1.27-alpine AS runtime

COPY deploy/docker/frontend.nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=builder /app/dist /usr/share/nginx/html

EXPOSE 80

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD wget -qO- http://127.0.0.1/ >/dev/null || exit 1
