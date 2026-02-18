"""CSRF Token API Endpoints."""

from __future__ import annotations

from fastapi import APIRouter, Request
from pydantic import BaseModel

from src.middleware.csrf import get_csrf_token

router = APIRouter(prefix="/csrf", tags=["CSRF"])


class CSRFTokenResponse(BaseModel):
    """CSRF token response model."""

    csrf_token: str


@router.get("/token", response_model=CSRFTokenResponse)
async def get_csrf(request: Request) -> CSRFTokenResponse:
    """
    Get CSRF token for current session.

    This endpoint should be called on page load or app initialization to obtain
    a CSRF token. The token is also automatically set in a cookie.

    **Usage:**
    1. Call this endpoint to get the CSRF token
    2. Store the token in your application state
    3. Include the token in the X-CSRF-Token header for all state-changing requests

    **Security:**
    - Token is set in a Secure, SameSite=Strict cookie
    - Token must match between cookie and request header
    - Protects against Cross-Site Request Forgery attacks

    Returns:
        CSRFTokenResponse: Object containing the CSRF token
    """
    token = await get_csrf_token(request)
    return CSRFTokenResponse(csrf_token=token)
