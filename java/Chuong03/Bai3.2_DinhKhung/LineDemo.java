import java.io.IOException;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;

/**
 * Bài 3.2 – Minh họa READ_LINE (Mã giả 3.5, Mục 3.4.3): định khung bằng ký tự xuống dòng.
 * Luồng "gửi" chia dữ liệu thành các mảnh cắt ngang dòng (kể cả giữa hai byte của chữ "à");
 * luồng chính đọc bằng READ_LINE. Mảnh cuối 100 byte không có '\n' để thấy lỗi "Dòng quá dài".
 * Chạy: java -cp out LineDemo [host] [port]      (mặc định 127.0.0.1 9000)
 */
public class LineDemo {
    static final int MAX_LEN = 64;

    static byte[] ascii(String s) { return s.getBytes(StandardCharsets.US_ASCII); }

    static byte[] concat(byte[] a, byte[] b) {
        byte[] r = Arrays.copyOf(a, a.length + b.length);
        System.arraycopy(b, 0, r, a.length, b.length);
        return r;
    }

    static final byte[][] PIECES = {
        ascii("HELLO\nWOR"),
        concat(ascii("LD\r\nXin ch"), new byte[] {(byte) 0xC3}),
        concat(new byte[] {(byte) 0xA0}, ascii("o\n")),
        ascii("A".repeat(100)),
    };

    /** Hiển thị dãy byte: ký tự ASCII in được giữ nguyên, còn lại dạng \xNN. */
    static String show(byte[] data) {
        StringBuilder sb = new StringBuilder();
        for (byte x : data) {
            int b = x & 0xFF;
            if (b == 0x0A) sb.append("\\n");
            else if (b == 0x0D) sb.append("\\r");
            else if (b >= 0x20 && b < 0x7F) sb.append((char) b);
            else sb.append(String.format("\\x%02x", b));
        }
        String t = sb.toString();
        return t.length() <= 24 ? t : t.substring(0, 21) + "...";
    }

    static void sender(String host, int port) {
        try (Socket s = NetUtil.safeConnect(host, port, 3000)) {
            s.setTcpNoDelay(true);                                          // NODELAY: mỗi mảnh đi riêng
            for (byte[] p : PIECES) {
                System.out.println("[gửi] " + p.length + " byte: " + show(p));
                Framing.sendAll(s, p);
                Thread.sleep(300);
            }
        } catch (IOException | InterruptedException e) {
            System.out.println("[gửi] Lỗi: " + e.getMessage());
        }
    }

    public static void main(String[] args) throws Exception {
        Object[] ep = NetUtil.parseEndpoint(args, "127.0.0.1", 9000);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        try (ServerSocket srv = new ServerSocket()) {
            srv.setReuseAddress(true);
            srv.bind(new InetSocketAddress(host, port), 1);
            System.out.println("[đọc] Đang lắng nghe tại " + host + ":" + port + ", max_len = " + MAX_LEN);
            Thread t = new Thread(() -> sender(host, port));
            t.setDaemon(true);
            t.start();
            Framing.Reader r = null;
            try (Socket conn = srv.accept()) {
                r = new Framing.Reader(conn);                               // Reader { sock, pending ← EMPTY_BYTES }
                while (true) {
                    String line = Framing.readLine(r, MAX_LEN);
                    if (line == null) {
                        System.out.println("[đọc] Đối tác đã đóng kết nối");
                        break;
                    }
                    System.out.println("[đọc] READ_LINE -> '" + line + "' (pending còn " + r.pending.length + " byte)");
                }
            } catch (Framing.ProtocolError e) {
                System.out.println("[đọc] ProtocolError: " + e.getMessage() + " (pending " + r.pending.length
                        + " byte > " + MAX_LEN + ")");
            }
            t.join();
        }
    }
}
