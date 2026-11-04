import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.security.MessageDigest;
import java.util.HexFormat;
import java.util.Map;
import java.util.concurrent.CompletableFuture;

/**
 * Bài 5.5 – Server nhận tệp có kiểm tra mã băm: Mã giả 3.9 (HANDLE_PUT, Mục 3.5.2) với khung JSON của
 * Mã giả 5.1; mỗi kết nối một luồng ảo. Ghi vào tệp tạm .part, chỉ đổi tên khi SHA-256 khớp; mọi lỗi
 * khi nhận (CATCH ANY) đều xóa tệp .part. rate_kib giới hạn tốc độ đọc (KiB/s) để mô phỏng đường truyền chậm.
 * Chạy: java -cp out PutServer [host] [port] [rate_kib]      (mặc định 0.0.0.0 9100 0 = không giới hạn)
 */
public class PutServer {
    static final long MAX_FILE = 100L * 1048576;
    static final String UPLOAD_DIR = "uploads";
    static final CompletableFuture<Void> NO_CANCEL = Khung.newCancellation();
    static int rateKiB;

    /** IS_HEX(v, n): v là chuỗi đúng n ký tự thập lục phân. */
    static boolean isHex(Object v, int n) {
        return v instanceof String t && t.length() == n && t.chars().allMatch(c -> Character.digit(c, 16) >= 0);
    }

    static void handlePut(Socket conn, Map<?, ?> req) throws Exception {
        String raw = (req.get("name") instanceof String t ? t : "").replace('\\', '/');   // dòng 3: REPLACE,
        String name = raw.substring(raw.lastIndexOf('/') + 1);                         // BASENAME
        long size = req.get("size") instanceof Long l ? l : -1;          // phải là số nguyên
        if (name.isEmpty() || name.equals(".") || name.equals("..") || size < 0 || size > MAX_FILE   // dòng 4–6
                || !isHex(req.get("sha256"), 64)) {
            Khung.sendJson(conn, Json.obj("status", 400, "error", "Sai tham số"), NO_CANCEL);
            return;
        }
        System.out.println("[server] Nhận yêu cầu put: " + name + ", " + size + " byte");
        Khung.sendJson(conn, Json.obj("status", 100), NO_CANCEL);   // sẵn sàng nhận nội dung
        Files.createDirectories(Path.of(UPLOAD_DIR));
        Path tmp = Path.of(UPLOAD_DIR, name + ".part");
        MessageDigest h = MessageDigest.getInstance("SHA-256");
        long remaining = size;
        byte[] buf = new byte[65536];
        InputStream in = conn.getInputStream();
        FileOutputStream f = new FileOutputStream(tmp.toFile());
        try {                                                        // dòng 14–26
            while (remaining > 0) {
                int n = in.read(buf, 0, (int) Math.min(65536, remaining));
                if (n == -1) throw new Khung.ConnectionClosed((int) (size - remaining));
                f.write(buf, 0, n);
                h.update(buf, 0, n);
                remaining -= n;
                if (rateKiB > 0) Thread.sleep((long) (n * 1000.0 / (rateKiB * 1024)));   // đường truyền chậm
            }
        } catch (Exception e) {                                      // CATCH ANY: xóa tệp dở dang rồi báo tiếp
            f.close();
            Files.deleteIfExists(tmp);
            System.out.println("[server] Lỗi khi mới nhận " + (size - remaining) + "/" + size
                    + " byte: xóa " + UPLOAD_DIR + "/" + name + ".part");
            throw e;
        }
        f.close();
        if (HexFormat.of().formatHex(h.digest()).equals(req.get("sha256"))) {
            Files.move(tmp, Path.of(UPLOAD_DIR, name), StandardCopyOption.REPLACE_EXISTING);
            System.out.println("[server] Đã lưu " + UPLOAD_DIR + "/" + name + ", mã băm khớp (200)");
            Khung.sendJson(conn, Json.obj("status", 200, "saved", name), NO_CANCEL);
        } else {
            Files.delete(tmp);
            System.out.println("[server] Mã băm không khớp (422)");
            Khung.sendJson(conn, Json.obj("status", 422, "error", "Mã băm không khớp"), NO_CANCEL);
        }
    }

    static void session(Socket conn) {
        String addr = conn.getInetAddress().getHostAddress() + ":" + conn.getPort();
        System.out.println("[server] Client kết nối: " + addr);
        try (conn) {
            while (true) {
                Object o;
                try {
                    o = Khung.recvJson(conn, NO_CANCEL);
                } catch (Khung.ConnectionClosed e) {
                    if (e.received == 0) break;
                    throw e;
                } catch (IllegalArgumentException e) {                 // Mã giả 3.7: JsonError → 400, tiếp tục
                    Khung.sendJson(conn, Json.obj("status", 400, "error", "JSON sai"), NO_CANCEL);
                    continue;
                }
                if (!(o instanceof Map<?, ?> req)) {                   // Mã giả 3.7 dòng 24–27
                    Khung.sendJson(conn, Json.obj("status", 400, "error", "JSON sai"), NO_CANCEL);
                    continue;
                }
                Object op = req.get("op");
                if ("put".equals(op)) handlePut(conn, req);
                else if ("bye".equals(op)) break;
                else Khung.sendJson(conn, Json.obj("status", 400, "error", "Sai thao tác"), NO_CANCEL);
            }
        } catch (Exception e) {
            System.out.println("[server] Lỗi với " + addr + ": " + e.getMessage());
        }
        System.out.println("[server] Client " + addr + " đã đóng kết nối");
    }

    public static void main(String[] args) throws IOException {
        Object[] ep = Khung.parseEndpoint(java.util.Arrays.copyOf(args, Math.min(args.length, 2)), "0.0.0.0", 9100);
        rateKiB = args.length > 2 ? Integer.parseInt(args[2]) : 0;
        Runtime.getRuntime().addShutdownHook(new Thread(() -> System.out.println("[server] Dừng server")));
        try (ServerSocket srv = new ServerSocket()) {
            srv.setReuseAddress(true);
            if (rateKiB > 0) srv.setReceiveBufferSize(65536);       // bộ đệm nhận nhỏ: mô phỏng đường truyền chậm
            srv.bind(new InetSocketAddress((String) ep[0], (Integer) ep[1]), 16);
            System.out.println("[server] Đang lắng nghe tại " + ep[0] + ":" + ep[1] + ", giới hạn đọc " + rateKiB + " KiB/s");
            while (true) {
                Socket conn = srv.accept();
                Thread.ofVirtual().start(() -> session(conn));
            }
        }
    }
}
