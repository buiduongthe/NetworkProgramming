import java.io.IOException;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.util.ArrayList;
import java.util.List;

/**
 * Bài 5.2 – Mở các cổng minh họa trên 127.0.0.1 để chạy thử chương trình quét.
 * - Cổng "mở": lắng nghe bình thường, chấp nhận rồi đóng ngay mọi kết nối.
 * - Cổng "bị lọc" (mô phỏng): lắng nghe với backlog 1 và không bao giờ accept; chương trình tự lấp đầy
 *   hàng đợi kết nối, sau đó Linux bỏ qua gói SYN mới nên phía quét hết thời gian chờ như khi gặp
 *   tường lửa. (Trên Windows hàng đợi đầy thường trả RST, tức cổng đóng.)
 * Chạy: java -cp out DemoPorts <cổng_mở,...> <cổng_lọc,...>
 */
public class DemoPorts {
    static final List<AutoCloseable> KEEP = new ArrayList<>();
    static final InetAddress LO = InetAddress.getLoopbackAddress();

    static ServerSocket listen(int port, int backlog) throws IOException {
        ServerSocket srv = new ServerSocket();
        srv.setReuseAddress(true);
        srv.bind(new InetSocketAddress(LO, port), backlog);
        KEEP.add(srv);
        return srv;
    }

    static void openPort(int port) throws IOException {
        ServerSocket srv = listen(port, 16);
        Thread.ofVirtual().start(() -> {
            while (true) {
                try { srv.accept().close(); } catch (IOException e) { return; }
            }
        });
    }

    static void filteredPort(int port) throws IOException {
        listen(port, 1);                                               // hàng đợi rất ngắn, không bao giờ accept
        for (int i = 0; i < 8; i++) {                                  // lấp đầy hàng đợi
            Socket c = new Socket();
            try {
                c.connect(new InetSocketAddress(LO, port), 300);
                KEEP.add(c);
            } catch (IOException e) {                                  // SYN bị bỏ qua: hàng đợi đã đầy
                c.close();
                break;
            }
        }
    }

    static List<Integer> parseList(String s) {
        List<Integer> r = new ArrayList<>();
        for (String x : s.split(",")) if (!x.isEmpty()) r.add(Integer.parseInt(x));
        return r;
    }

    public static void main(String[] args) throws Exception {
        List<Integer> opened = parseList(args.length > 0 ? args[0] : "9003,9011,9017");
        List<Integer> filtered = parseList(args.length > 1 ? args[1] : "9005,9008,9013,9019");
        for (int p : opened) openPort(p);
        for (int p : filtered) filteredPort(p);
        System.out.println("[demo] Cổng mở: " + String.join(", ", opened.stream().map(String::valueOf).toList()));
        System.out.println("[demo] Cổng bị lọc (mô phỏng): " + String.join(", ", filtered.stream().map(String::valueOf).toList()));
        System.out.println("[demo] Nhấn Ctrl+C để dừng");
        Runtime.getRuntime().addShutdownHook(new Thread(() -> System.out.println("[demo] Dừng")));
        Thread.sleep(Long.MAX_VALUE);
    }
}
