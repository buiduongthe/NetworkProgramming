import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.net.SocketException;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.Locale;

/**
 * Bài mở rộng MR3.1 – Client thử: gửi dữ liệu, đóng chiều gửi, đọc phản hồi tới EOF.
 *
 * Chạy: java -cp out ClientThu <host> <port> <chuỗi> [số_lần_lặp]
 */
public class ClientThu {
    public static void main(String[] args) throws IOException {
        String host = args[0], text = args[2];
        int port = Integer.parseInt(args[1]);
        int repeat = args.length > 3 ? Integer.parseInt(args[3]) : 1;
        byte[] data = text.repeat(repeat).getBytes(StandardCharsets.UTF_8);
        ByteArrayOutputStream chunks = new ByteArrayOutputStream();
        try (Socket s = new Socket()) {
            s.connect(new InetSocketAddress(host, port), 5000);
            s.setSoTimeout(5000);
            try {
                s.getOutputStream().write(data);
                s.shutdownOutput();                      // báo hết dữ liệu: server mới bắt đầu trả lời
                InputStream in = s.getInputStream();
                byte[] buf = new byte[65536];
                int n;
                while ((n = in.read(buf)) != -1)         // -1 = EOF: phía bên kia đã đóng chiều gửi
                    chunks.write(buf, 0, n);
            } catch (SocketException e) {                // bên kia đóng khi còn dữ liệu chưa đọc -> RST
                if (e.getMessage() == null || !e.getMessage().contains("reset")) throw e;
                System.out.println("[client] Kết nối bị đặt lại (RST) trước khi có phản hồi");
            }
        }
        byte[] reply = chunks.toByteArray();
        System.out.println("[client] Gửi " + data.length + " byte, nhận " + reply.length + " byte");
        if (reply.length == 0) {
            System.out.println("[client] Kết nối bị đóng mà không có dữ liệu trả về");
            System.exit(1);
        }
        String all = new String(reply, StandardCharsets.UTF_8);
        String preview = all.codePoints().limit(40)
                .collect(StringBuilder::new, StringBuilder::appendCodePoint, StringBuilder::append).toString();
        boolean ok = Arrays.equals(reply, text.toUpperCase(Locale.ROOT).repeat(repeat).getBytes(StandardCharsets.UTF_8));
        System.out.println("[client] Phản hồi: '" + preview + "'" + (all.codePointCount(0, all.length()) > 40 ? "..." : ""));
        System.out.println("[client] So khớp với bản chữ hoa: " + (ok ? "KHỚP" : "SAI"));
    }
}
