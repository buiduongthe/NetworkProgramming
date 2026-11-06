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
    /** Windows: JVM đổi tham số dòng lệnh sang bảng mã ANSI của hệ thống (thường cp1252) nên chữ Việt như "ở", "ượ"
     *  thành "?". Đọc lại dòng lệnh gốc (UTF-16) của tiến trình, tách theo quy tắc của Windows và lấy args.length
     *  tham số cuối. Hệ điều hành khác (hoặc không đọc được) giữ nguyên args. */
    static String[] thamSoUnicode(String[] args) {
        if (args.length == 0 || !System.getProperty("os.name", "").startsWith("Windows")) return args;
        String dong = ProcessHandle.current().info().commandLine().orElse(null);
        if (dong == null) return args;
        java.util.List<String> ds = new java.util.ArrayList<>();
        StringBuilder sb = new StringBuilder();
        boolean trongNhay = false, co = false;
        for (int i = 0; i < dong.length(); ) {
            char c = dong.charAt(i);
            if (c == '\\') {                                    // n dấu \ trước " -> n/2 dấu \ (+ " nếu n lẻ)
                int n = 0;
                while (i < dong.length() && dong.charAt(i) == '\\') { n++; i++; }
                boolean nhay = i < dong.length() && dong.charAt(i) == '"';
                sb.append("\\".repeat(nhay ? n / 2 : n));
                if (nhay && n % 2 == 1) { sb.append('"'); i++; }
                co = true;
            } else if (c == '"') {
                if (trongNhay && i + 1 < dong.length() && dong.charAt(i + 1) == '"') { sb.append('"'); i += 2; }
                else { trongNhay = !trongNhay; i++; }
                co = true;
            } else if ((c == ' ' || c == '\t') && !trongNhay) {
                if (co) { ds.add(sb.toString()); sb.setLength(0); co = false; }
                i++;
            } else { sb.append(c); co = true; i++; }
        }
        if (co) ds.add(sb.toString());
        if (ds.size() < args.length) return args;
        String[] kq = ds.subList(ds.size() - args.length, ds.size()).toArray(new String[0]);
        for (int k = 0; k < args.length; k++)                      // tách lệch (tham số không hỏng mà khác) -> giữ nguyên
            if (!args[k].equals(kq[k]) && !args[k].contains("?")) return args;
        return kq;
    }

    public static void main(String[] args) throws IOException {
        args = thamSoUnicode(args);
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
