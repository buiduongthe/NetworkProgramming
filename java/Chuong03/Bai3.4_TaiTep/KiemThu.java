import java.io.IOException;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HexFormat;
import java.util.List;
import java.util.Map;

/**
 * Bài 3.4 – Client kiểm thử chức năng tải tệp (ca 8, 9 của tập ca kiểm thử, Mục 3.5.3): tên chứa "../"
 * hoặc "..\", tên chỉ gồm thư mục, mã băm sai, kích thước vượt MAX_FILE, ngắt khi đang gửi nội dung.
 * Mỗi ca mở một kết nối riêng tới server.
 * Chạy: java -cp out KiemThu [host] [port]      (mặc định 127.0.0.1 9100)
 */
public class KiemThu {
    static String host;
    static int port;
    static final byte[] DATA = "Nội dung thử\n".repeat(10).getBytes(StandardCharsets.UTF_8);
    static final List<Boolean> results = new ArrayList<>();

    static Socket connect() throws IOException {
        Socket s = NetUtil.safeConnect(host, port, 3000);
        s.setSoTimeout(5000);
        return s;
    }

    static void report(String no, String name, boolean ok, String detail) {
        results.add(ok);
        System.out.println("[kiểm thử] Ca " + no + " – " + name + ": " + (ok ? "ĐẠT" : "KHÔNG ĐẠT") + " (" + detail + ")");
    }

    static long status(Map<?, ?> r) { return r.get("status") instanceof Long v ? v : -1; }

    /** Gửi tiêu đề put; nếu server sẵn sàng (100) thì gửi nội dung và trả phản hồi cuối. */
    static Map<?, ?> putRaw(Socket s, String name, byte[] data, String sha256, long size) throws Exception {
        if (sha256 == null) sha256 = HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(data));
        Framing.sendJson(s, Json.obj("op", "put", "name", name, "size", size < 0 ? data.length : size, "sha256", sha256));
        Map<?, ?> resp = (Map<?, ?>) Framing.recvJson(s);
        if (status(resp) != 100) return resp;
        Framing.sendAll(s, data);
        return (Map<?, ?>) Framing.recvJson(s);
    }

    public static void main(String[] args) throws Exception {
        Object[] ep = NetUtil.parseEndpoint(args, "127.0.0.1", 9100);
        host = (String) ep[0];
        port = (Integer) ep[1];
        String zeros = "0".repeat(64);
        System.out.println("[kiểm thử] Server " + host + ":" + port);
        try (Socket s = connect()) {
            Map<?, ?> r = putRaw(s, "../thu.txt", DATA, null, -1);                   // Ca 8a
            report("8a", "tên '../thu.txt'", status(r) == 200 && "thu.txt".equals(r.get("saved")),
                    "status " + status(r) + ", lưu '" + r.get("saved") + "'");
            r = putRaw(s, "..\\..\\thu2.txt", DATA, null, -1);                       // Ca 8b
            report("8b", "tên '..\\..\\thu2.txt'", status(r) == 200 && "thu2.txt".equals(r.get("saved")),
                    "status " + status(r) + ", lưu '" + r.get("saved") + "'");
            r = putRaw(s, "../", DATA, null, -1);                                    // Ca 8c
            report("8c", "tên '../' (không còn tên tệp)", status(r) == 400,
                    "bị từ chối: status " + status(r) + ", " + r.get("error"));
            r = putRaw(s, "sai_bam.txt", DATA, zeros, -1);                           // Ca 9
            report("9", "mã băm khai báo sai", status(r) == 422, "status " + status(r) + ", " + r.get("error"));
            r = putRaw(s, "lon.bin", DATA, null, 200L * 1048576);                    // Ca bổ sung 1
            report("bổ sung 1", "size 200 MiB > MAX_FILE", status(r) == 400, "status " + status(r) + ", " + r.get("error"));
            Framing.sendJson(s, Json.obj("op", "bye"));
        }
        Map<?, ?> r100;
        try (Socket s = connect()) {                                                 // Ca bổ sung 2: ngắt giữa chừng
            Framing.sendJson(s, Json.obj("op", "put", "name", "dang_do.txt", "size", 1000, "sha256", zeros));
            r100 = (Map<?, ?>) Framing.recvJson(s);
            Framing.sendAll(s, Arrays.copyOf(DATA, 100));                            // chỉ gửi 100/1000 byte rồi đóng
        }
        try (Socket s = connect()) {
            Framing.sendJson(s, Json.obj("op", "list"));
            Map<?, ?> r2 = (Map<?, ?>) Framing.recvJson(s);
            report("bổ sung 2", "ngắt sau 100/1000 byte", status(r100) == 100 && status(r2) == 200,
                    "server vẫn phục vụ client mới, list -> " + status(r2));
            Framing.sendJson(s, Json.obj("op", "bye"));
        }
        long ok = results.stream().filter(x -> x).count();
        System.out.println("[kiểm thử] Kết quả: " + ok + "/" + results.size() + " ca đạt");
    }
}
