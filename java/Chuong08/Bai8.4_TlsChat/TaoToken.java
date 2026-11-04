import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * Bài 8.4 – Cấp token JWT thử nghiệm (vai trò server ủy quyền giản lược, Mục 7.2.4); khóa ký: CHAT_JWT_KEY.
 * Chạy: java -cp out TaoToken <sub> <tệp_token> [aud=chat] [thời_hạn_giây=3600]
 */
public class TaoToken {
    public static void main(String[] args) throws IOException {
        PrintStream out = new PrintStream(new FileOutputStream(FileDescriptor.out), true, StandardCharsets.UTF_8);
        if (args.length < 2) { out.println("Cách dùng: java -cp out TaoToken <sub> <tệp_token> [aud] [thời_hạn_giây]"); System.exit(1); }
        String sub = args[0], aud = args.length > 2 ? args[2] : "chat";
        Path path = Path.of(args[1]);
        long ttl = args.length > 3 ? Long.parseLong(args[3]) : 3600, now = System.currentTimeMillis() / 1000;
        String token = Jwt.sign(Json.obj("iss", Jwt.ISSUER, "sub", sub, "aud", aud, "iat", now, "exp", now + ttl),
                Jwt.secret("CHAT_JWT_KEY"));
        if (path.toAbsolutePath().getParent() != null) Files.createDirectories(path.toAbsolutePath().getParent());
        Files.writeString(path, token, StandardCharsets.US_ASCII);
        out.println("[token] Đã cấp token sub=" + sub + ", aud=" + aud + ", iss=" + Jwt.ISSUER + ", hết hạn sau " + ttl + " s -> " + args[1]);
    }
}
