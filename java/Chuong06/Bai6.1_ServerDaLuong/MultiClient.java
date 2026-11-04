import java.io.DataInputStream;
import java.io.IOException;
import java.net.Socket;
import java.net.SocketException;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/**
 * Bài 6.1 – Mở nhiều client tra cứu đồng thời để quan sát server đa luồng và nhóm luồng.
 * Mỗi client chạy trong một luồng, thực hiện LOOKUP_CLIENT (Mã giả 3.8) rút gọn: gửi lookup "TCP",
 * in phản hồi kèm thời điểm t, giữ kết nối hold_ms rồi gửi bye. Các client khởi động cách nhau 100 ms.
 * Chạy: java -cp out MultiClient [host] [port] [n] [hold_ms]   (mặc định 127.0.0.1 9000 5 1000)
 */
public class MultiClient {
    static long t0;

    static long t() { return (System.nanoTime() - t0) / 1_000_000; }

    public static void main(String[] args) throws InterruptedException {
        Object[] ep = NetUtil.parseEndpoint(args, "127.0.0.1", 9000);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        int n = NetUtil.intArg(args, 2, 5);
        int holdMs = NetUtil.intArg(args, 3, 1000);
        System.out.println("[client] " + n + " client cách nhau 100 ms, giữ kết nối " + holdMs + " ms");
        t0 = System.nanoTime();
        List<Thread> threads = new ArrayList<>();
        for (int i = 1; i <= n; i++) {
            int id = i;
            Thread th = new Thread(() -> oneClient(id, host, port, holdMs));
            th.start();
            threads.add(th);
            Thread.sleep(100);
        }
        for (Thread th : threads) th.join();
        System.out.println("[client] Hoàn tất sau " + t() + " ms");
    }

    static void oneClient(int i, String host, int port, int holdMs) {
        Socket s;
        try {
            s = NetUtil.safeConnect(host, port, 3000);
        } catch (IOException e) {
            System.out.println("[client " + i + "] Lỗi: " + e.getMessage());
            return;
        }
        try (s) {
            s.setSoTimeout(5000);                                       // RECV_TIMEOUT 5000 ms
            DataInputStream in = new DataInputStream(s.getInputStream());
            NetUtil.sendJson(s.getOutputStream(), Json.obj("op", "lookup", "term", "TCP"));
            Map<String, Object> resp = NetUtil.recvJson(in);
            Object status = resp.get("status");
            if (!Long.valueOf(200).equals(status)) {
                System.out.println("[client " + i + "] t = " + t() + " ms: bị từ chối " + status + " – " + resp.get("error"));
                return;
            }
            System.out.println("[client " + i + "] t = " + t() + " ms: 200 " + resp.get("term") + " – " + resp.get("text"));
            Thread.sleep(holdMs);                                       // giữ kết nối
            NetUtil.sendJson(s.getOutputStream(), Json.obj("op", "bye"));
        } catch (NetUtil.ConnectionClosed e) {
            System.out.println("[client " + i + "] t = " + t() + " ms: server đóng kết nối ngay");
        } catch (SocketException e) {
            if ("Connection reset".equals(e.getMessage()))              // server đóng ngay sau ACCEPT
                System.out.println("[client " + i + "] t = " + t() + " ms: server đóng kết nối ngay");
            else
                System.out.println("[client " + i + "] Lỗi: " + e.getMessage());
        } catch (IOException | InterruptedException e) {
            System.out.println("[client " + i + "] Lỗi: " + e.getMessage());
        }
    }
}
