import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.HexFormat;
import java.util.Map;

/**
 * Bài 3.4 – Server tra cứu và nhận tệp có kiểm tra SHA-256 (Mã giả 3.9, Mục 3.5.2). HANDLE_PUT được gắn vào
 * HANDLE_LOOKUP (Mã giả 3.7) bằng nhánh  IF req.op = "put" THEN HANDLE_PUT(conn, req); CONTINUE.
 * An toàn: BASENAME chặn "../"; giới hạn MAX_FILE; ghi tệp tạm .part, chỉ đổi tên khi mã băm khớp.
 * Chạy: java -cp out PutServer [host] [port]   (mặc định 0.0.0.0 9100; tệp lưu trong thư mục uploads/)
 */
public class PutServer {
    static final long MAX_FILE = 100L * 1048576;
    static final String UPLOAD_DIR = "uploads";
    static Map<?, ?> dict = Map.of();                                     // DICT

    /** Mã giả 3.7 – nghiệp vụ tra cứu, độc lập với socket. */
    static Map<String, Object> processRequest(Object req) {
        String op = str(req, "op");
        if ("lookup".equals(op)) {
            String t = str(req, "term");
            if (t != null && dict.containsKey(t)) return Json.obj("status", 200, "term", t, "text", dict.get(t));
            return Json.obj("status", 404, "error", "Không có thuật ngữ");
        }
        if ("list".equals(op)) return Json.obj("status", 200, "terms", new ArrayList<>(dict.keySet()));
        return Json.obj("status", 400, "error", "Sai thao tác");
    }

    static String str(Object o, String key) {
        return o instanceof Map<?, ?> m && m.get(key) instanceof String s ? s : null;
    }

    /** IS_HEX(v, n): v là chuỗi đúng n ký tự thập lục phân. */
    static boolean isHex(String v, int n) {
        return v != null && v.length() == n && v.chars().allMatch(c -> Character.digit(c, 16) >= 0);
    }

    /** BASENAME(REPLACE(name, "\\", "/")): đổi '\' thành '/' rồi bỏ mọi thành phần thư mục. */
    static String baseName(String name) {
        if (name == null) return "";
        String n = name.replace('\\', '/');
        return n.substring(n.lastIndexOf('/') + 1);
    }

    /** Mã giả 3.9 – req = {op, name, size, sha256}; trả chuỗi kết quả để ghi nhật ký. */
    static String handlePut(Socket conn, Map<?, ?> req) throws IOException, NoSuchAlgorithmException {
        String name = baseName(str(req, "name"));                          // dòng 3: bỏ thư mục, chặn "../"
        long size = req.get("size") instanceof Long z ? z : -1;
        String sha = str(req, "sha256");
        if (name.isEmpty() || name.equals(".") || name.equals("..") || size < 0 || size > MAX_FILE   // dòng 4–6
                || !isHex(sha, 64)) {
            Framing.sendJson(conn, Json.obj("status", 400, "error", "Sai tham số"));
            return "400 (Sai tham số)";
        }
        Framing.sendJson(conn, Json.obj("status", 100));                    // sẵn sàng nhận nội dung
        Path tmp = Path.of(UPLOAD_DIR, name + ".part");
        OutputStream f = Files.newOutputStream(tmp);                        // OPEN_FILE(tmp, WRITE)
        MessageDigest h = MessageDigest.getInstance("SHA-256");             // SHA256_INIT()
        InputStream in = conn.getInputStream();
        long remaining = size;
        byte[] buf = new byte[65536];
        try {
            while (remaining > 0) {
                int n = in.read(buf, 0, (int) Math.min(65536, remaining));
                if (n == -1)
                    throw new Framing.ConnectionClosed((int) (size - remaining),
                            "Tệp chưa nhận đủ (" + (size - remaining) + "/" + size + " byte)");
                f.write(buf, 0, n);
                h.update(buf, 0, n);                                        // SHA256_UPDATE
                remaining -= n;
            }
        } catch (Exception e) {                                             // dòng 23–25 CATCH ANY: xóa tệp dở dang
            f.close();
            Files.deleteIfExists(tmp);
            throw e;
        }
        f.close();                                                          // CLOSE_FILE(f)
        if (HexFormat.of().formatHex(h.digest()).equals(sha)) {             // HEX(SHA256_FINAL(h))
            Files.move(tmp, Path.of(UPLOAD_DIR, name), StandardCopyOption.REPLACE_EXISTING);   // RENAME
            Framing.sendJson(conn, Json.obj("status", 200, "saved", name));
            return "200, lưu " + UPLOAD_DIR + "/" + name + " (" + size + " byte)";
        }
        Files.delete(tmp);
        Framing.sendJson(conn, Json.obj("status", 422, "error", "Mã băm không khớp"));
        return "422 (Mã băm không khớp)";
    }

    /** Mã giả 3.7 – HANDLE_LOOKUP, bổ sung nhánh "put" theo Mã giả 3.9. */
    static void handleLookup(Socket conn, String addr) throws IOException, NoSuchAlgorithmException {
        while (true) {
            Object req;
            try {
                req = Framing.recvJson(conn);
            } catch (Framing.ConnectionClosed e) {
                if (e.received == 0) return;                                // đóng tại ranh giới
                throw e;                                                    // đóng giữa khung: lỗi
            } catch (Json.JsonError e) {
                Framing.sendJson(conn, Json.obj("status", 400, "error", "JSON sai"));
                continue;
            }
            if (!(req instanceof Map)) {                                    // Mã giả 3.7 dòng 24–27
                Framing.sendJson(conn, Json.obj("status", 400, "error", "JSON sai"));
                continue;
            }
            String op = str(req, "op") == null ? "?" : str(req, "op");
            if (op.equals("bye")) return;
            if (op.equals("put")) {
                Map<?, ?> m = (Map<?, ?>) req;
                System.out.println("[server] " + addr + ": put '" + m.get("name") + "' -> " + handlePut(conn, m));
                continue;
            }
            Map<String, Object> resp = processRequest(req);
            Framing.sendJson(conn, resp);
            String term = op.equals("lookup") ? " '" + ((Map<?, ?>) req).get("term") + "'" : "";
            System.out.println("[server] " + addr + ": " + op + term + " -> " + resp.get("status"));
        }
    }

    public static void main(String[] args) throws Exception {
        Object[] ep = NetUtil.parseEndpoint(args, "0.0.0.0", 9100);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        dict = (Map<?, ?>) Json.parse(Files.readString(Path.of("glossary.json")));  // LOAD_JSON_FILE
        Files.createDirectories(Path.of(UPLOAD_DIR));
        try (ServerSocket srv = new ServerSocket()) {                      // SOCKET(INET, STREAM)
            srv.setReuseAddress(true);                                      // SETOPT(REUSEADDR)
            srv.bind(new InetSocketAddress(host, port), 16);                // BIND + LISTEN(16)
            System.out.println("[server] Đang lắng nghe tại " + host + ":" + port + ", thư mục tải lên: " + UPLOAD_DIR + "/");
            while (true) {
                try (Socket conn = srv.accept()) {                          // ACCEPT; try-with-resources = CLOSE
                    String addr = conn.getInetAddress().getHostAddress() + ":" + conn.getPort();
                    System.out.println("[server] Client kết nối: " + addr);
                    try {
                        handleLookup(conn, addr);
                    } catch (IOException e) {      // NetError, ConnectionClosed, ProtocolError: không dừng server
                        System.out.println("[server] Lỗi với " + addr + ": " + e.getMessage());
                    }
                    System.out.println("[server] Client " + addr + " đã đóng kết nối");
                }
            }
        }
    }
}
