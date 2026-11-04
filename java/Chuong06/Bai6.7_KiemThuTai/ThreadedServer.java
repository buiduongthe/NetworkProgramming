import java.io.IOException;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * Bài 6.7 – Bản sao server mỗi kết nối một luồng của Bài 6.1 (Mã giả 6.1, Mục 6.2.1) để kiểm thử tải.
 * Khác Bài 6.1: không ghi nhật ký từng kết nối; tham số delay_ms là độ trễ giả lập của PROCESS_REQUEST.
 * Biên dịch: javac -encoding UTF-8 -d out *.java
 * Chạy:      java -cp out ThreadedServer [host] [port] [max_clients] [delay_ms]   (mặc định 0.0.0.0 9000 500 50)
 */
public class ThreadedServer {
    static final int MAX_CLIENTS = 500;                                 // CONST MAX_CLIENTS ← 500

    public static void main(String[] args) throws IOException {
        Object[] ep = NetUtil.parseEndpoint(args, "0.0.0.0", 9000);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        int maxClients = NetUtil.intArg(args, 2, MAX_CLIENTS);
        LookupService.delayMs = NetUtil.intArg(args, 3, 50);
        try (ServerSocket srv = new ServerSocket()) {                  // SOCKET(INET, STREAM)
            srv.setReuseAddress(true);                                  // SETOPT(REUSEADDR)
            srv.bind(new InetSocketAddress(host, port), 128);           // BIND + LISTEN(128)
            System.out.println("[server] Đang lắng nghe tại " + host + ":" + port);
            System.out.println("[server] Mỗi kết nối một luồng, MAX_CLIENTS = " + maxClients
                    + ", độ trễ " + LookupService.delayMs + " ms");
            AtomicInteger active = new AtomicInteger(0);                // ATOMIC_COUNTER(0)
            while (true) {
                Socket conn = srv.accept();                             // ACCEPT
                String addr = NetUtil.addr(conn);
                if (active.get() >= maxClients) {                       // MỚI: giới hạn
                    System.out.println("[server] Từ chối " + addr + ": đã đủ " + maxClients + " kết nối");
                    conn.close();
                    continue;
                }
                active.incrementAndGet();
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
        }
    }
}
