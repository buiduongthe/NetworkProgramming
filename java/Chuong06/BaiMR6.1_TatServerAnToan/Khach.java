import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.Locale;

/**
 * Bài mở rộng MR6.1 – Client thử cho server tắt an toàn.
 *
 * Gửi lần lượt các yêu cầu "LAM <ms>", in phản hồi; sau đó tiếp tục giữ kết nối và in mọi
 * dòng server gửi (BYE, HUY) cho tới khi server đóng kết nối.
 *
 * Chạy: java -cp out Khach <tên> <host> <port> [ms ...]
 */
public class Khach {
    public static void main(String[] args) throws IOException {
        String name = args[0], host = args[1];
        int port = Integer.parseInt(args[2]);
        String tag = "[khach " + name + "]";
        Socket s = new Socket();
        try {
            s.connect(new InetSocketAddress(host, port), 3000);
        } catch (IOException e) {
            System.out.println(tag + " Không kết nối được: " + e.getMessage());
            System.exit(1);
        }
        s.setSoTimeout(10000);
        long t0 = System.nanoTime();
        try (s; BufferedReader f = new BufferedReader(new InputStreamReader(s.getInputStream(), StandardCharsets.UTF_8))) {
            for (int i = 3; i < args.length; i++) {
                s.getOutputStream().write(("LAM " + args[i] + "\n").getBytes(StandardCharsets.UTF_8));
                System.out.println(tag + " Gửi LAM " + args[i]);
                String reply = f.readLine();
                if (reply == null) {
                    System.out.println(tag + " Server đóng kết nối khi chưa trả lời");
                    return;
                }
                System.out.println(tag + String.format(Locale.ROOT, " t=%.1fs nhận: ", (System.nanoTime() - t0) / 1e9) + reply.strip());
            }
            String line;
            while ((line = f.readLine()) != null)                       // giữ kết nối, chờ thông báo của server
                System.out.println(tag + String.format(Locale.ROOT, " t=%.1fs nhận: ", (System.nanoTime() - t0) / 1e9) + line.strip());
            System.out.println(tag + String.format(Locale.ROOT, " t=%.1fs server đóng kết nối", (System.nanoTime() - t0) / 1e9));
        }
    }
}
