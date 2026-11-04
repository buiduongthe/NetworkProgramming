"""Bài 8.4 – Cấp và kiểm tra JWT ký HMAC-SHA-256 (HS256) cho xác thực người dùng chat.

VERIFY_JWT của Mã giả 8.6 theo Mục 7.2.4: kiểm tra đúng thuật toán (từ chối alg "none"), chữ ký,
exp chưa quá hạn, iss là ISSUER, aud là đối tượng mong đợi. Khóa ký lấy qua SECRET("CHAT_JWT_KEY").
Chạy (cấp token thử nghiệm, vai trò server ủy quyền giản lược):
    CHAT_JWT_KEY=<khóa> python3 jwt_hs256.py <sub> <tệp_token> [aud=chat] [thời_hạn_giây=3600]
"""
import base64
import hashlib
import hmac
import json
import os
import sys
import time

ISSUER = "auth.lab.local"


class JwtError(Exception):
    pass


def secret(name):
    value = os.environ.get(name)
    if not value:
        raise KeyError(f"Thiếu biến môi trường {name}")
    return value


def b64url(b):
    return base64.urlsafe_b64encode(b).rstrip(b"=").decode("ascii")


def b64url_decode(s):
    return base64.urlsafe_b64decode(s + "=" * (-len(s) % 4))


def sign_jwt(claims, key):
    head = b64url(json.dumps({"alg": "HS256", "typ": "JWT"}, separators=(",", ":")).encode())
    body = b64url(json.dumps(claims, separators=(",", ":"), ensure_ascii=False).encode("utf-8"))
    sig = hmac.new(key.encode("utf-8"), f"{head}.{body}".encode("ascii"), hashlib.sha256).digest()
    return f"{head}.{body}.{b64url(sig)}"


def verify_jwt(token, key, issuer, audience):
    """VERIFY_JWT(token, issuer, audience) -> claims; lỗi -> JwtError kèm lý do."""
    parts = token.split(".") if isinstance(token, str) else []
    if len(parts) != 3:
        raise JwtError("định dạng sai")
    try:
        header = json.loads(b64url_decode(parts[0]))
        claims = json.loads(b64url_decode(parts[1]))
        sig = b64url_decode(parts[2])
    except ValueError:
        raise JwtError("định dạng sai")
    if header.get("alg") != "HS256":                              # đúng thuật toán mong đợi
        raise JwtError("thuật toán không được chấp nhận")
    expect = hmac.new(key.encode("utf-8"), f"{parts[0]}.{parts[1]}".encode("ascii"), hashlib.sha256).digest()
    if not hmac.compare_digest(sig, expect):                      # so sánh thời gian hằng
        raise JwtError("chữ ký sai")
    if not isinstance(claims.get("exp"), (int, float)) or claims["exp"] <= time.time():
        raise JwtError("đã hết hạn")
    if claims.get("iss") != issuer:
        raise JwtError("iss không đúng")
    aud = claims.get("aud")
    if audience not in (aud if isinstance(aud, list) else [aud]):
        raise JwtError("aud không đúng")
    if not isinstance(claims.get("sub"), str) or not claims["sub"]:
        raise JwtError("thiếu sub")
    return claims


def main():
    if len(sys.argv) < 3:
        sys.exit("Cách dùng: python3 jwt_hs256.py <sub> <tệp_token> [aud] [thời_hạn_giây]")
    sub, path = sys.argv[1], sys.argv[2]
    aud = sys.argv[3] if len(sys.argv) > 3 else "chat"
    ttl = int(sys.argv[4]) if len(sys.argv) > 4 else 3600
    now = int(time.time())
    token = sign_jwt({"iss": ISSUER, "sub": sub, "aud": aud, "iat": now, "exp": now + ttl}, secret("CHAT_JWT_KEY"))
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    with open(path, "w", encoding="ascii") as f:
        f.write(token)
    print(f"[token] Đã cấp token sub={sub}, aud={aud}, iss={ISSUER}, hết hạn sau {ttl} s -> {path}")


if __name__ == "__main__":
    main()
