import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.HexFormat;
import java.util.Map;

/**
 * Bài 3.2 – Echo Client dùng khung JSON: Mã giả 3.2 (Mục 3.2.3) với vòng gửi nhận thô thay bằng
 * SEND_JSON/RECV_JSON của Mã giả 3.6. Tham số thứ ba (tùy chọn) là số ký tự của thông điệp lớn
 * gửi sau khi hết dữ liệu nhập, ví dụ 200000.
 * Chạy: java -cp out FrameClient [host] [port] [so_ky_tu]      (mặc định 127.0.0.1 9000 0)
 */
public class FrameClient {
    static final String PATTERN = "Mạng máy tính ";                       // 14 ký tự, 18 byte UTF-8

    static String makeText(int n) {                                        // chuỗi đúng n ký tự có chữ tiếng Việt
        StringBuilder sb = new StringBuilder();
        while (sb.length() < n) sb.append(PATTERN);
        return sb.substring(0, n);
    }

    static String sha256Hex(String text) throws NoSuchAlgorithmException {
        byte[] d = MessageDigest.getInstance("SHA-256").digest(text.getBytes(StandardCharsets.UTF_8));
        return HexFormat.of().formatHex(d);
    }

    static int utf8Len(String s) { return s.getBytes(StandardCharsets.UTF_8).length; }

    public static void main(String[] args) throws Exception {
        String[] ep2 = java.util.Arrays.copyOf(args, Math.min(args.length, 2));
        Object[] ep = NetUtil.parseEndpoint(ep2, "127.0.0.1", 9000);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        int big = args.length > 2 ? Integer.parseInt(args[2]) : 0;
        try (Socket s = NetUtil.safeConnect(host, port, 3000)) {
            s.setSoTimeout(5000);                                           // RECV_TIMEOUT 5000 ms
            System.out.println("[client] Đã kết nối " + host + ":" + port
                    + ". Nhập dòng cần gửi (Ctrl+Z/Ctrl+D để kết thúc).");
            BufferedReader console = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
            String line;
            while ((line = console.readLine()) != null) {                   // READ_CONSOLE()
                Framing.sendJson(s, Json.obj("text", line));                // SEND_JSON thay cho SEND_ALL thô
                Object resp = Framing.recvJson(s);                          // RECV_JSON: luôn nhận đủ một khung
                Object text = resp instanceof Map<?, ?> m ? m.get("text") : null;
                System.out.println("[client] Gửi: '" + line + "' -> Nhận: '" + text + "'");
            }
            if (big > 0) {
                String text = makeText(big);
                System.out.println("[client] Gửi thông điệp lớn: " + text.length() + " ký tự, "
                        + utf8Len(text) + " byte UTF-8");
                Framing.sendJson(s, Json.obj("text", text));
                Object resp = Framing.recvJson(s);
                String back = resp instanceof Map<?, ?> m && m.get("text") instanceof String t ? t : "";
                boolean same = back.equals(text);
                System.out.println("[client] Nhận lại: " + back.length() + " ký tự, " + utf8Len(back)
                        + " byte; trùng khớp: " + (same ? "có" : "không"));
                System.out.println("[client] SHA-256 gửi:  " + sha256Hex(text).substring(0, 32) + "…");
                System.out.println("[client] SHA-256 nhận: " + sha256Hex(back).substring(0, 32) + "…");
            }
            s.shutdownOutput();                                             // SHUTDOWN(WRITE)
        }                                                                   // try-with-resources = CLOSE
        System.out.println("[client] Đã đóng kết nối");
    }
}
