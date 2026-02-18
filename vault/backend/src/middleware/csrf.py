"""
CSRF Protection Middleware

Implements Cross-Site Request Forgery protection using the Double Submit Cookie pattern.
"""

import hmac
import secrets

from fastapi import HTTPException, Request, status
from starlette.types import ASGIApp, Receive, Scope, Send

CSRF_TOKEN_LENGTH = 32
CSRF_COOKIE_NAME = "csrf_token"
CSRF_HEADER_NAME = "X-CSRF-Token"
CSRF_FORM_FIELD = "csrf_token"
CSRF_SAFE_METHODS = {"GET", "HEAD", "OPTIONS", "TRACE"}

CSRF_EXEMPT_PATHS = {
    "/api/v1/auth/register",
    "/api/v1/auth/token",
    "/api/v1/health",
    "/api/docs",
    "/api/redoc",
    "/api/openapi.json",
}


class CSRFError(HTTPException):
    """Raised when CSRF validation fails"""

    def __init__(self, detail: str) -> None:
        super().__init__(status_code=status.HTTP_403_FORBIDDEN, detail=detail)


def generate_csrf_token() -> str:
    """Generate a cryptographically secure CSRF token."""
    return secrets.token_hex(CSRF_TOKEN_LENGTH)


def verify_csrf_token(token_from_cookie: str | None, token_from_request: str | None) -> bool:
    """Verify CSRF token using constant-time comparison."""
    if not token_from_cookie or not token_from_request:
        return False

    if len(token_from_cookie) != len(token_from_request):
        return False

    return hmac.compare_digest(token_from_cookie, token_from_request)


async def get_csrf_token(request: Request) -> str:
    """Get or generate CSRF token for current request."""
    token = request.cookies.get(CSRF_COOKIE_NAME)

    if not token:
        token = generate_csrf_token()
        request.state.new_csrf_token = token

    return token


async def csrf_protect(request: Request) -> None:
    """CSRF protection dependency for FastAPI endpoints."""
    if request.method in CSRF_SAFE_METHODS:
        return

    path = request.url.path
    if path in CSRF_EXEMPT_PATHS:
        return

    token_from_cookie = request.cookies.get(CSRF_COOKIE_NAME)
    token_from_header = request.headers.get(CSRF_HEADER_NAME)
    token_from_request = token_from_header

    if not token_from_request and request.method == "POST":
        try:
            form = await request.form()
            token_from_request = form.get(CSRF_FORM_FIELD)
        except Exception:
            pass

    if not token_from_cookie:
        raise CSRFError("CSRF token missing in cookie. Please refresh the page.")

    if not token_from_request:
        raise CSRFError(f"CSRF token required in header '{CSRF_HEADER_NAME}' or request body")

    if not verify_csrf_token(token_from_cookie, token_from_request):
        raise CSRFError("CSRF token validation failed. Token may be expired or invalid.")


class CSRFMiddleware:
    """ASGI Middleware to automatically set CSRF token cookie on responses."""

    def __init__(
        self,
        app: ASGIApp,
        cookie_secure: bool = True,
        cookie_samesite: str = "Strict",
        cookie_httponly: bool = False,
        cookie_domain: str | None = None,
    ) -> None:
        self.app = app
        self.cookie_secure = cookie_secure
        self.cookie_samesite = cookie_samesite
        self.cookie_httponly = cookie_httponly
        self.cookie_domain = cookie_domain

    async def __call__(self, scope: Scope, receive: Receive, send: Send) -> None:
        if scope["type"] != "http":
            await self.app(scope, receive, send)
            return

        request = Request(scope, receive)

        token = request.cookies.get(CSRF_COOKIE_NAME)
        if not token:
            token = generate_csrf_token()

        if hasattr(request.state, "new_csrf_token"):
            token = request.state.new_csrf_token

        async def send_with_csrf(message: dict) -> None:
            if message["type"] == "http.response.start":
                headers = list(message.get("headers", []))

                has_csrf_cookie = any(
                    name.lower() == b"set-cookie" and CSRF_COOKIE_NAME.encode() in value
                    for name, value in headers
                )

                if not has_csrf_cookie:
                    cookie_parts = [
                        f"{CSRF_COOKIE_NAME}={token}",
                        "Path=/",
                        f"SameSite={self.cookie_samesite}",
                    ]

                    if self.cookie_secure:
                        cookie_parts.append("Secure")

                    if self.cookie_httponly:
                        cookie_parts.append("HttpOnly")

                    if self.cookie_domain:
                        cookie_parts.append(f"Domain={self.cookie_domain}")

                    cookie_value = "; ".join(cookie_parts)
                    headers.append((b"set-cookie", cookie_value.encode()))
                    message["headers"] = headers

            await send(message)

        await self.app(scope, receive, send_with_csrf)
