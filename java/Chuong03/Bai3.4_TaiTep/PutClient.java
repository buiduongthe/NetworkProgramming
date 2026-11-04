import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.NoSuchFileException;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.HexFormat;
import java.util.Map;

/**
 * Bài 3.4 – Client tra cứu và tải tệp lên server: Mã giả 3.10 (PUT_FILE, Mục 3.5.2) trên nền client
 * tra cứu của Mã giả 3.8; một kết nối cho cả tra cứu và tải tệp. Mỗi dòng nhập là một lệnh:
 *   put <đường dẫn tệp>  – tải tệp lên;   <thuật ngữ>  – tra cứu.
 * Chạy: java -cp out PutClient [host] [port]      (mặc định 127.0.0.1 9100)
 */
public class PutClient {
    static String sha256OfFile(Path path) throws Exception {
        MessageDigest h = MessageDigest.getInstance("SHA-256");
        try (InputStream f = Files.newInputStream(path)) {
            byte[] buf = new byte[65536];
            int n;
            while ((n = f.read(buf)) > 0) h.update(buf, 0, n);
        }
        return HexFormat.of().formatHex(h.digest());
    }

    /** Mã giả 3.10: khung tiêu đề -> chờ 100 -> gửi đúng size byte -> nhận kết quả. */
    static void putFile(Socket s, Path path) throws Exception {
        String digest = sha256OfFile(path);                                 // HEX(SHA256_OF_FILE(path))
        String name = path.getFileName().toString();
        long size = Files.size(path);
        System.out.println("[client] Tải lên " + name + ": " + size + " byte, SHA-256 " + digest.substring(0, 16) + "…");
        Framing.sendJson(s, Json.obj("op", "put", "name", name, "size", size, "sha256", digest));
        Map<?, ?> resp = (Map<?, ?>) Framing.recvJson(s);
        if (!Long.valueOf(100).equals(resp.get("status"))) throw new Framing.ProtocolError(String.valueOf(resp.get("error")));
        try (InputStream f = Files.newInputStream(path)) {                  // OPEN_FILE(path, READ)
            byte[] buf = new byte[65536];
            int n;
            while ((n = f.read(buf)) > 0)                                   // READ(f, 65536)
                Framing.sendAll(s, java.util.Arrays.copyOf(buf, n));
        }
        resp = (Map<?, ?>) Framing.recvJson(s);
        System.out.println("[client] Kết quả: " + resp.get("status") + " " + Json.stringify(resp));
    }

    public static void main(String[] args) throws Exception {
        Object[] ep = NetUtil.parseEndpoint(args, "127.0.0.1", 9100);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        try (Socket s = NetUtil.safeConnect(host, port, 3000)) {
            s.setSoTimeout(5000);                                           // RECV_TIMEOUT 5000 ms
            System.out.println("[client] Đã kết nối " + host + ":" + port
                    + ". Nhập 'put <tệp>' hoặc thuật ngữ (Ctrl+Z/Ctrl+D để kết thúc).");
            BufferedReader console = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
            String line;
            while ((line = console.readLine()) != null) {                   // READ_CONSOLE()
                if (line.startsWith("put ")) {
                    String path = line.substring(4).trim();
                    try {
                        putFile(s, Path.of(path));
                    } catch (NoSuchFileException e) {
                        System.out.println("[client] Lỗi: không tìm thấy tệp '" + path + "'");
                    } catch (Framing.ProtocolError e) {                     // server không trả 100
                        System.out.println("[client] Lỗi: " + e.getMessage());
                    }
                    continue;
                }
                Framing.sendJson(s, Json.obj("op", "lookup", "term", line));
                Map<?, ?> resp = (Map<?, ?>) Framing.recvJson(s);
                if (Long.valueOf(200).equals(resp.get("status")))
                    System.out.println("[client] " + resp.get("term") + ": " + resp.get("text"));
                else
                    System.out.println("[client] Lỗi " + resp.get("status") + ": " + resp.get("error") + " ('" + line + "')");
            }
            Framing.sendJson(s, Json.obj("op", "bye"));
        } catch (IOException e) {
            System.out.println("[client] Lỗi: " + e.getMessage());
        }
        System.out.println("[client] Đã đóng kết nối");
    }
}
