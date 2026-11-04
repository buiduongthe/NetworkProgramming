import java.io.IOException;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * Bài 6.1 – Server tra cứu đa luồng: mỗi kết nối một luồng (Mã giả 6.1, Mục 6.2.1).
 * Kế thừa Mã giả 3.1 (khung server); nghiệp vụ là HANDLE_LOOKUP của Mã giả 3.7.
 * Biên dịch: javac -encoding UTF-8 -d out *.java
 * Chạy:      java -cp out ThreadedServer [host] [port] [max_clients]   (mặc định 0.0.0.0 9000 500)
 */
public class ThreadedServer {
    static final int MAX_CLIENTS = 500;                                 // CONST MAX_CLIENTS ← 500

    public static void main(String[] args) throws IOException {
        Object[] ep = NetUtil.parseEndpoint(args, "0.0.0.0", 9000);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        int maxClients = NetUtil.intArg(args, 2, MAX_CLIENTS);
        try (ServerSocket srv = new ServerSocket()) {                  // SOCKET(INET, STREAM)
            srv.setReuseAddress(true);                                  // SETOPT(REUSEADDR)
            srv.bind(new InetSocketAddress(host, port), 128);           // BIND + LISTEN(128)
            System.out.println("[server] Đang lắng nghe tại " + host + ":" + port);
            System.out.println("[server] Mỗi kết nối một luồng, MAX_CLIENTS = " + maxClients);
            AtomicInteger active = new AtomicInteger(0);                // ATOMIC_COUNTER(0)
            while (true) {
                Socket conn = srv.accept();                             // ACCEPT
                String addr = NetUtil.addr(conn);
                if (active.get() >= maxClients) {                       // MỚI: giới hạn
                    System.out.println("[server] Từ chối " + addr + ": đã đủ " + maxClients + " kết nối");
                    conn.close();
                    continue;
                }
                int n = active.incrementAndGet();
                System.out.println("[server] Nhận " + addr + ", tạo luồng mới (đang phục vụ: " + n + ")");
                new Thread(() -> serve(conn, addr, active)).start();   // SPAWN_THREAD (luồng hệ điều hành)
            }
        }
    }

    /** SERVE – khối TRY của Mã giả 3.1, chạy trong luồng riêng của mỗi kết nối. */
    static void serve(Socket conn, String addr, AtomicInteger active) {
        try {
            conn.setSoTimeout(300000);                                  // RECV_TIMEOUT: đóng kết nối nhàn rỗi
            LookupService.handleLookup(conn);
        } catch (IOException e) {                                       // NetError, Timeout, ProtocolError
            System.out.println("[server] Lỗi với " + addr + ": " + e.getMessage());
        } finally {
            try { conn.close(); } catch (IOException ignored) { }
            active.decrementAndGet();                                   // luôn giảm bộ đếm, kể cả khi lỗi
            System.out.println("[server] Client " + addr + " đã đóng kết nối");
        }
    }
}
