import java.nio.charset.StandardCharsets;
import java.security.GeneralSecurityException;
import java.security.MessageDigest;
import java.util.Base64;
import java.util.List;
import java.util.Map;
import javax.crypto.Mac;
import javax.crypto.spec.SecretKeySpec;

/**
 * Bài 8.4 – Cấp và kiểm tra JWT ký HMAC-SHA-256 (HS256). VERIFY_JWT của Mã giả 8.6 theo Mục 7.2.4:
 * đúng thuật toán (từ chối alg "none"), chữ ký, exp chưa quá hạn, iss là ISSUER, aud là đối tượng mong đợi.
 */
public class Jwt {
    public static final String ISSUER = "auth.lab.local";

    public static class JwtError extends Exception { JwtError(String m) { super(m); } }

    /** SECRET(name): bí mật chỉ lấy từ biến môi trường. */
    public static String secret(String name) {
        String v = System.getenv(name);
        if (v == null || v.isEmpty()) throw new IllegalStateException("Thiếu biến môi trường " + name);
        return v;
    }

    static String b64(byte[] b) { return Base64.getUrlEncoder().withoutPadding().encodeToString(b); }

    static byte[] mac(String key, String data) {
        try {
            Mac m = Mac.getInstance("HmacSHA256");
            m.init(new SecretKeySpec(key.getBytes(StandardCharsets.UTF_8), "HmacSHA256"));
            return m.doFinal(data.getBytes(StandardCharsets.US_ASCII));
        } catch (GeneralSecurityException e) { throw new IllegalStateException(e); }
    }

    public static String sign(Map<String, Object> claims, String key) {
        String head = b64("{\"alg\":\"HS256\",\"typ\":\"JWT\"}".getBytes(StandardCharsets.UTF_8));
        String body = b64(Json.stringify(claims).getBytes(StandardCharsets.UTF_8));
        return head + "." + body + "." + b64(mac(key, head + "." + body));
    }

    @SuppressWarnings("unchecked")
    public static Map<String, Object> verify(String token, String key, String issuer, String audience) throws JwtError {
        String[] parts = token == null ? new String[0] : token.split("\\.", -1);
        if (parts.length != 3) throw new JwtError("định dạng sai");
        Map<String, Object> header, claims;
        byte[] sig;
        try {
            Base64.Decoder dec = Base64.getUrlDecoder();
            header = (Map<String, Object>) Json.parse(new String(dec.decode(parts[0]), StandardCharsets.UTF_8));
            claims = (Map<String, Object>) Json.parse(new String(dec.decode(parts[1]), StandardCharsets.UTF_8));
            sig = dec.decode(parts[2]);
        } catch (IllegalArgumentException | ClassCastException e) { throw new JwtError("định dạng sai"); }
        if (!"HS256".equals(header.get("alg"))) throw new JwtError("thuật toán không được chấp nhận");
        if (!MessageDigest.isEqual(sig, mac(key, parts[0] + "." + parts[1])))   // so sánh thời gian hằng
            throw new JwtError("chữ ký sai");
        if (!(claims.get("exp") instanceof Number exp) || exp.doubleValue() <= System.currentTimeMillis() / 1000.0)
            throw new JwtError("đã hết hạn");
        if (!issuer.equals(claims.get("iss"))) throw new JwtError("iss không đúng");
        Object aud = claims.get("aud");
        boolean audOk = aud instanceof List<?> l ? l.contains(audience) : audience.equals(aud);
        if (!audOk) throw new JwtError("aud không đúng");
        if (!(claims.get("sub") instanceof String sub) || sub.isEmpty()) throw new JwtError("thiếu sub");
        return claims;
    }
}
