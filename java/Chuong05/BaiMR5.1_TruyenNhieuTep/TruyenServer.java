import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.io.RandomAccessFile;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.Map;

/**
 * Bài mở rộng MR5.1 – Server truyền nhiều tệp: mỗi kết nối phục vụ một lượt tải xuống/tải lên.
 *
 * Giao thức: nhận một khung JSON điều khiển, rồi truyền luồng byte (xem Khung.java).
 *  - download: {"op":"download","name":TÊN,"offset":OFF}
 *       -> trả {"ok":true,"size":TỔNG,"sha256":HEX} rồi gửi byte [OFF..hết].
 *  - upload:   {"op":"upload","name":TÊN,"offset":OFF,"size":TỔNG,"sha256":HEX}
 *       -> trả {"ok":true} rồi nhận byte, ghi nối vào TÊN.part từ OFF; xong thì kiểm SHA-256,
 *          đổi tên thành TÊN, trả {"ok":true,"sha256_ok":...}.
 * Server KHÔNG tự giới hạn tốc độ – client điều tiết, TCP tự tạo áp lực ngược.
 *
 * Chạy: java -cp out TruyenServer <host> <port> [thư_mục_lưu]
 */
public class TruyenServer {
    static long num(Object o) { return o == null ? 0 : ((Number) o).longValue(); }

    static void phucVu(Socket conn, String kho) {
        try (conn) {
            InputStream in = conn.getInputStream();
            OutputStream out = conn.getOutputStream();
            Map<String, Object> req = Khung.nhanKhung(in);
            Object op = req.get("op");
            String name = new File(String.valueOf(req.getOrDefault("name", ""))).getName();   // chỉ lấy tên tệp, chặn ../
            if ("download".equals(op)) {
                File path = new File(kho, name);
                if (!path.isFile()) {
                    Khung.guiKhung(conn, Json.obj("ok", false, "loi", "Không có tệp " + name));
                    return;
                }
                Khung.guiKhung(conn, Json.obj("ok", true, "size", path.length(), "sha256", Khung.sha256Tep(path.getPath())));
                long off = num(req.get("offset"));
                try (RandomAccessFile f = new RandomAccessFile(path, "r")) {
                    f.seek(off);
                    byte[] buf = new byte[Khung.CHUNK];
                    int n;
                    while ((n = f.read(buf)) > 0) out.write(buf, 0, n);   // SEND_ALL
                }
            } else if ("upload".equals(op)) {
                long off = num(req.get("offset"));
                long total = num(req.get("size"));
                File part = new File(kho, name + ".part");
                Khung.guiKhung(conn, Json.obj("ok", true));
                try (RandomAccessFile f = new RandomAccessFile(part, "rw")) {
                    if (!(off > 0)) f.setLength(0);               // tải mới: ghi đè từ đầu
                    f.seek(off);
                    long nhan = off;
                    while (nhan < total) {
                        byte[] b = Khung.docDu(in, (int) Math.min(Khung.CHUNK, total - nhan));
                        f.write(b);
                        nhan += b.length;
                    }
                }
                boolean shaOk = Khung.sha256Tep(part.getPath()).equals(req.get("sha256"));
                if (shaOk) Files.move(part.toPath(), Path.of(kho, name), StandardCopyOption.REPLACE_EXISTING);
                Khung.guiKhung(conn, Json.obj("ok", true, "sha256_ok", shaOk));
            } else {
                Khung.guiKhung(conn, Json.obj("ok", false, "loi", "op lạ"));
            }
        } catch (IOException e) {
            // client ngắt giữa chừng (pause/stop) là bình thường
        }
    }

    public static void main(String[] args) throws IOException {
        if (args.length < 2) {
            System.err.println("Cách dùng: java -cp out TruyenServer <host> <port> [thư_mục_lưu]");
            System.exit(1);
        }
        String host = args[0];
        int port = Integer.parseInt(args[1]);
        String kho = args.length > 2 ? args[2] : "kho_server";
        Files.createDirectories(Path.of(kho));
        Runtime.getRuntime().addShutdownHook(new Thread(() -> System.out.println("[server] Dừng.")));
        try (ServerSocket srv = new ServerSocket()) {
            srv.setReuseAddress(true);
            srv.bind(new InetSocketAddress(host, port), 16);
            System.out.println("[server] Nghe tại " + host + ":" + port + ", thư mục lưu: " + kho);
            while (true) {
                Socket conn = srv.accept();
                Thread t = new Thread(() -> phucVu(conn, kho));
                t.setDaemon(true);
                t.start();
            }
        }
    }
}
