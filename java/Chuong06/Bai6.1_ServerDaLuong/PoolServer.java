import java.io.IOException;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * Bài 6.1 – Server tra cứu dùng nhóm luồng và hàng đợi có giới hạn (Mã giả 6.2, Mục 6.2.2).
 * Kế thừa Mã giả 6.1: dùng lại nguyên SERVE; k luồng thợ dùng chung một hàng đợi; hàng đợi đầy → 503.
 * Chạy: java -cp out PoolServer [host] [port] [k] [queue_size]   (mặc định 0.0.0.0 9000 8 16)
 */
public class PoolServer {
    record Job(Socket conn, String addr) {}

    public static void main(String[] args) throws IOException {
        Object[] ep = NetUtil.parseEndpoint(args, "0.0.0.0", 9000);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        int k = NetUtil.intArg(args, 2, 8);
        int queueSize = NetUtil.intArg(args, 3, 16);
        try (ServerSocket srv = new ServerSocket()) {                  // như Mã giả 6.1
            srv.setReuseAddress(true);
            srv.bind(new InetSocketAddress(host, port), 128);
            BlockingQueue<Job> q = new ArrayBlockingQueue<>(queueSize); // BLOCKING_QUEUE(queue_size)
            for (int i = 1; i <= k; i++) {                              // FOR i ← 1 TO k: SPAWN_THREAD(WORKER, q)
                int id = i;
                Thread t = new Thread(() -> worker(id, q));
                t.setDaemon(true);
                t.start();
            }
            System.out.println("[server] Đang lắng nghe tại " + host + ":" + port);
            System.out.println("[server] Nhóm " + k + " luồng thợ, hàng đợi " + queueSize + " chỗ");
            while (true) {
                Socket conn = srv.accept();                             // ACCEPT
                String addr = NetUtil.addr(conn);
                System.out.println("[server] Nhận " + addr);
                if (!q.offer(new Job(conn, addr))) {                    // q.try_put: hàng đợi đầy
                    System.out.println("[server] Hàng đợi đầy, trả 503 cho " + addr);
                    try {
                        NetUtil.sendJson(conn.getOutputStream(), Json.obj("status", 503, "error", "Server bận"));
                    } catch (IOException ignored) { }                   // client đã đi: bỏ qua
                    conn.close();
                }
            }
        }
    }

    /** WORKER: lấy kết nối khỏi hàng đợi rồi phục vụ bằng SERVE. */
    static void worker(int id, BlockingQueue<Job> q) {
        while (true) {
            Job job;
            try {
                job = q.take();                                         // chờ đến khi có việc
            } catch (InterruptedException e) {
                return;
            }
            System.out.println("[server] Luồng thợ " + id + " phục vụ " + job.addr());
            ThreadedServer.serve(job.conn(), job.addr(), new AtomicInteger(1));   // bộ đếm tạm
        }
    }
}
