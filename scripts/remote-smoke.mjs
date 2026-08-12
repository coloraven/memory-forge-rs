const DEFAULT_REMOTE_URL = "http://127.0.0.1:7331";
const REQUEST_TIMEOUT_MS = 8000;
const DUPLICATE_SLASHES = /\/{2,}/g;
const TRAILING_SLASHES = /\/+$/;

function parseBaseUrl() {
  const args = process.argv.slice(2);
  let cliUrl;
  if (args.length === 1 && !args[0].startsWith("--")) {
    [cliUrl] = args;
  } else if (args.length === 1 && args[0].startsWith("--url=")) {
    cliUrl = args[0].slice("--url=".length);
  } else if (args.length === 2 && args[0] === "--url") {
    cliUrl = args[1];
  } else if (args.length > 0) {
    throw new Error("Usage: npm run test:remote -- [http://host:port]");
  }

  const configured =
    cliUrl ?? process.env.MEMORY_FORGE_REMOTE_URL ?? DEFAULT_REMOTE_URL;
  const url = new URL(configured);
  if (!["http:", "https:"].includes(url.protocol)) {
    throw new Error("Remote URL must use HTTP or HTTPS");
  }
  if (url.username || url.password || url.search || url.hash) {
    throw new Error(
      "Remote URL must not contain credentials, a query, or a fragment"
    );
  }
  url.pathname = url.pathname.replace(TRAILING_SLASHES, "");
  return url;
}

function endpoint(baseUrl, path) {
  const url = new URL(baseUrl);
  url.pathname = `${url.pathname}/${path}`.replace(DUPLICATE_SLASHES, "/");
  return url;
}

function request(baseUrl, path, token) {
  const headers = { "x-request-id": `host-smoke-${crypto.randomUUID()}` };
  if (token) {
    headers.authorization = `Bearer ${token}`;
  }
  return fetch(endpoint(baseUrl, path), {
    headers,
    redirect: "error",
    signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
  });
}

async function responseJson(response, label) {
  try {
    return await response.json();
  } catch {
    throw new Error(`${label} did not return JSON`);
  }
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

function pass(message) {
  console.log(`[PASS] ${message}`);
}

async function main() {
  const baseUrl = parseBaseUrl();
  const token = process.env.MEMORY_FORGE_REMOTE_TOKEN?.trim();
  console.log(`Remote smoke target: ${baseUrl.origin}${baseUrl.pathname}`);

  const healthResponse = await request(baseUrl, "health");
  assert(
    healthResponse.status === 200,
    `/health returned HTTP ${healthResponse.status}`
  );
  const health = await responseJson(healthResponse, "/health");
  assert(health.status === "ok", "/health did not report status=ok");
  assert(health.protocolVersion === 1, "/health protocol version is not v1");
  assert(
    healthResponse.headers.get("x-content-type-options") === "nosniff",
    "/health is missing X-Content-Type-Options: nosniff"
  );
  assert(
    healthResponse.headers
      .get("content-security-policy")
      ?.includes("default-src 'self'"),
    "/health is missing the expected Content-Security-Policy"
  );
  pass("public health and security headers");

  const bootstrapResponse = await request(baseUrl, "api/v1/bootstrap");
  assert(
    bootstrapResponse.status === 200,
    `/api/v1/bootstrap returned HTTP ${bootstrapResponse.status}`
  );
  const bootstrap = await responseJson(bootstrapResponse, "/api/v1/bootstrap");
  assert(
    bootstrap.protocolVersion === 1,
    "bootstrap envelope is not protocol v1"
  );
  assert(
    bootstrap.data?.protocolVersion === 1,
    "bootstrap payload is not protocol v1"
  );
  assert(
    typeof bootstrap.data?.auth?.required === "boolean",
    "bootstrap auth policy is missing"
  );
  pass("public bootstrap contract");

  const unauthenticated = await request(baseUrl, "api/v1/dashboard");
  if (bootstrap.data.auth.required) {
    assert(
      unauthenticated.status === 401,
      `unauthenticated dashboard returned HTTP ${unauthenticated.status}, expected 401`
    );
    const unauthenticatedBody = await responseJson(
      unauthenticated,
      "unauthenticated dashboard"
    );
    assert(
      unauthenticatedBody.error?.code === "AUTH_REQUIRED",
      "unauthenticated dashboard did not return AUTH_REQUIRED"
    );
    assert(
      unauthenticated.headers.get("www-authenticate")?.startsWith("Bearer "),
      "unauthenticated dashboard is missing the Bearer challenge"
    );
    pass("protected snapshot rejects missing credentials");

    assert(
      token,
      "MEMORY_FORGE_REMOTE_TOKEN is required because this host requires authentication"
    );
    const authenticated = await request(baseUrl, "api/v1/dashboard", token);
    assert(
      authenticated.status === 200,
      `authenticated dashboard returned HTTP ${authenticated.status}`
    );
    const authenticatedBody = await responseJson(
      authenticated,
      "authenticated dashboard"
    );
    assert(
      authenticatedBody.protocolVersion === 1,
      "authenticated response is not protocol v1"
    );
    assert(
      authenticatedBody.data,
      "authenticated dashboard payload is missing"
    );
    pass("Bearer-authenticated dashboard snapshot");
  } else {
    assert(
      unauthenticated.status === 200,
      `loopback dashboard returned HTTP ${unauthenticated.status}`
    );
    const dashboard = await responseJson(unauthenticated, "loopback dashboard");
    assert(
      dashboard.protocolVersion === 1,
      "dashboard response is not protocol v1"
    );
    assert(dashboard.data, "dashboard payload is missing");
    pass("loopback dashboard snapshot");
  }

  console.log("Remote smoke checks passed.");
}

main().catch((error) => {
  const message = error instanceof Error ? error.message : String(error);
  console.error(`[FAIL] ${message}`);
  process.exitCode = 1;
});
