import java.io.DataInputStream;
import java.io.IOException;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Random;

/**
 * Bài 6.3 – Nhiều client echo đồng thời để kiểm tra server reactor.
 * Mỗi client (một luồng) dựa trên Mã giả 3.2 nhưng nhận bằng RECV_EXACT (Mã giả 3.4): gửi một dòng chào,
 * nhận lại đúng số byte; sau đó gửi một khối lớn ở luồng phụ, đồng thời nhận lại đủ khối và so khớp.
 * Cuối cùng gửi thêm 200 000 byte rồi đóng chiều gửi và đọc đến hết: server phải gửi nốt rồi mới đóng.
 * Khối lớn hơn MAX_BUF của server nên kiểm tra được việc gửi một phần và áp lực ngược.
 * Chạy: java -cp out EchoClients [host] [port] [n] [size]   (mặc định 127.0.0.1 9000 3 1048576)
 */
public class EchoClients {
    static synchronized void log(String msg) { System.out.println(msg); }

    public static void main(String[] args) throws InterruptedException {
        Object[] ep = NetUtil.parseEndpoint(args, "127.0.0.1", 9000);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        int n = NetUtil.intArg(args, 2, 3);
        int size = NetUtil.intArg(args, 3, 1048576);
        System.out.println("[client] Mở " + n + " kết nối đồng thời, mỗi kết nối gửi thêm " + size + " byte");
        long t0 = System.nanoTime();
        List<Thread> threads = new ArrayList<>();
        for (int i = 1; i <= n; i++) {
            int id = i;
            threads.add(new Thread(() -> oneClient(id, host, port, size)));
        }
        for (Thread t : threads) t.start();
        for (Thread t : threads) t.join();
        System.out.println("[client] Hoàn tất sau " + (System.nanoTime() - t0) / 1_000_000 + " ms");
    }

    static final int TAIL = 200000;     // khối cuối gửi trước khi đóng chiều gửi (lớn hơn MAX_BUF của server)

    static void oneClient(int i, String host, int port, int size) {
        Socket s;
        try {
            s = NetUtil.safeConnect(host, port, 3000);
        } catch (IOException e) {
            log("[client " + i + "] Lỗi: " + e.getMessage());
            return;
        }
        try (s) {
            s.setSoTimeout(10000);
            DataInputStream in = new DataInputStream(s.getInputStream());
            byte[] hello = ("Xin chào reactor từ client " + i).getBytes(StandardCharsets.UTF_8);
            s.getOutputStream().write(hello);                           // write gửi hết (SEND_ALL)
            byte[] back = NetUtil.recvExact(in, hello.length);
            log("[client " + i + "] Nhận lại: '" + new String(back, StandardCharsets.UTF_8) + "'");
            byte[] block = new byte[size];
            new Random().nextBytes(block);
            Thread sender = new Thread(() -> {                          // gửi song song với nhận
                try {
                    s.getOutputStream().write(block);
                } catch (IOException e) {
                    log("[client " + i + "] Lỗi gửi: " + e.getMessage());
                }
            });
            sender.start();
            byte[] got = NetUtil.recvExact(in, size);
            sender.join();
            log("[client " + i + "] Gửi " + size + " byte, nhận lại " + got.length + " byte, khớp: "
                    + (Arrays.equals(got, block) ? "có" : "không"));
            byte[] tail = new byte[TAIL];                               // phần 3: gửi khối cuối rồi đóng chiều gửi
            new Random().nextBytes(tail);
            Thread sender2 = new Thread(() -> {
                try {
                    s.getOutputStream().write(tail);
                    s.shutdownOutput();                                 // SHUTDOWN(s, WRITE)
                } catch (IOException e) {
                    log("[client " + i + "] Lỗi gửi: " + e.getMessage());
                }
            });
            sender2.start();
            byte[] rest = in.readAllBytes();                            // đọc đến FIN của server
            sender2.join();
            log("[client " + i + "] Gửi " + TAIL + " byte rồi SHUTDOWN(WRITE), nhận lại " + rest.length
                    + " byte trước FIN của server, khớp: " + (Arrays.equals(rest, tail) ? "có" : "không"));
        } catch (IOException | InterruptedException e) {
            log("[client " + i + "] Lỗi: " + e.getMessage());
        }
    }
}
