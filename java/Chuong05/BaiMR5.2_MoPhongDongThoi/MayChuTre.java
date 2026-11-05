import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.TimeUnit;

/**
 * Máy chủ thử có độ trễ giả lập (chỉ chạy cục bộ). Giao thức dòng văn bản trên TCP: client gửi
 * "GET <k>\n", máy chủ chờ <trễ> ms rồi trả "OK <k>\n"; mỗi kết nối một luồng ảo. Sau mỗi "đợt"
 * yêu cầu (máy chủ rỗi 300 ms), in số yêu cầu và số kết nối đồng thời lớn nhất của đợt.
 *
 * Chạy: java -cp out DongThoi server <host> <port> [trễ_ms]
 */
final class MayChuTre {
    private MayChuTre() {}

    private static int active, peak, count, total, burst;           // bảo vệ bằng khóa của lớp (synchronized)
    private static final ScheduledExecutorService timer = Executors.newSingleThreadScheduledExecutor(r -> {
        Thread t = new Thread(r);
        t.setDaemon(true);
        return t;
    });

    static void log(String s) { System.out.println("[server] " + s); }

    /** Hẹn 300 ms sau kiểm tra: nếu vẫn rỗi thì đợt yêu cầu vừa rồi đã kết thúc. */
    private static void reportWhenIdle() {
        timer.schedule(() -> {
            synchronized (MayChuTre.class) {
                if (active == 0 && count > 0) {
                    burst++;
                    log(String.format("đợt %2d: %2d yêu cầu, đồng thời tối đa %2d", burst, count, peak));
                    count = 0;
                    peak = 0;
                }
            }
        }, 300, TimeUnit.MILLISECONDS);
    }

    static void handle(Socket c, int delayMs) {
        synchronized (MayChuTre.class) {
            active++;
            count++;
            total++;
            peak = Math.max(peak, active);
        }
        try (c) {
            String line = new BufferedReader(new InputStreamReader(c.getInputStream(), StandardCharsets.UTF_8)).readLine();
            if (line != null) {
                Thread.sleep(delayMs);                                // giả lập độ trễ
                String k = line.strip().startsWith("GET ") ? line.strip().substring(4) : line.strip();
                OutputStream out = c.getOutputStream();
                out.write(("OK " + k + "\n").getBytes(StandardCharsets.UTF_8));
            }
        } catch (IOException | InterruptedException ignored) { }
        synchronized (MayChuTre.class) {
            active--;
            if (active == 0) reportWhenIdle();
        }
    }

    static void run(String host, int port, int delayMs) throws IOException {
        ServerSocket ln = new ServerSocket();
        ln.setReuseAddress(true);
        ln.bind(new InetSocketAddress(host, port), 128);
        log("lắng nghe " + host + ":" + port + ", mỗi yêu cầu trễ " + delayMs + " ms");
        Runtime.getRuntime().addShutdownHook(new Thread(() -> {     // Ctrl+C
            synchronized (MayChuTre.class) { log("dừng; đã phục vụ " + total + " yêu cầu"); }
        }));
        while (true) {
            Socket c = ln.accept();
            Thread.ofVirtual().start(() -> handle(c, delayMs));     // mỗi kết nối một luồng ảo
        }
    }
}
