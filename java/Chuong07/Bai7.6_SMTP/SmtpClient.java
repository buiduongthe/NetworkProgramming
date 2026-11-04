import java.io.FileInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.security.KeyStore;
import java.security.cert.CertificateFactory;
import java.time.ZoneOffset;
import java.time.ZonedDateTime;
import java.time.format.DateTimeFormatter;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Base64;
import java.util.List;
import java.util.Locale;
import java.util.stream.Collectors;
import javax.net.ssl.SSLContext;
import javax.net.ssl.SSLException;
import javax.net.ssl.SSLParameters;
import javax.net.ssl.SSLSocket;
import javax.net.ssl.SSLSocketFactory;
import javax.net.ssl.TrustManagerFactory;

/**
 * Bài 7.6 – Phiên gửi thư SMTP có STARTTLS và AUTH PLAIN (Mã giả 7.7, Mục 7.5.1 Giáo trình Lập trình mạng).
 * Kế thừa: Mã giả 2.2 (SAFE_CONNECT), 3.5 (READ_LINE). Tài khoản lấy từ biến môi trường SMTP_USER, SMTP_PASS.
 * Biên dịch: javac -encoding UTF-8 -d out *.java
 * Chạy:      java -cp out SmtpClient [host] [port] [tệp_CA]   (mặc định localhost 587 cert.pem; "-" = kho mặc định)
 */
public class SmtpClient {
    static class SmtpException extends IOException {
        SmtpException(int code, List<String> text) { super(code + " " + String.join(" ", text)); }
    }

    record Reply(int code, List<String> text) {}

    public static void main(String[] args) throws Exception {
        String host = args.length >= 1 ? args[0] : "localhost";
        int port = args.length >= 2 ? Integer.parseInt(args[1]) : 587;
        String caFile = args.length >= 3 ? args[2] : "cert.pem";
        String user = System.getenv().getOrDefault("SMTP_USER", ""), secret = System.getenv().getOrDefault("SMTP_PASS", "");
        String from = "sinhvien@example.com", to = "giangvien@example.com";
        try {
            sendMail(host, port, caFile, user, secret, from, to, buildMessage(from, to));
            System.out.println("[client] Gửi thư thành công");
        } catch (SmtpException e) {
            System.out.println("[client] Lỗi SMTP: " + e.getMessage());
            System.exit(1);
        } catch (java.net.ProtocolException e) {
            System.out.println("[client] Lỗi giao thức: " + e.getMessage());
            System.exit(1);
        } catch (SSLException e) {                                      // chứng chỉ không tin cậy hoặc sai tên máy
            System.out.println("[client] Lỗi TLS: " + e.getMessage());
            System.exit(1);
        }
    }

    /** Mã giả 7.7, dòng 1–11: đọc phản hồi nhiều dòng ("250-..." ... "250 ..."). */
    static Reply smtpReply(Reader r) throws IOException {
        List<String> text = new ArrayList<>();
        while (true) {
            String l = r.readLine(1024);
            if (l == null || l.length() < 3)                            // dòng 5: server đóng hoặc dòng quá ngắn
                throw new java.net.ProtocolException("Phản hồi SMTP không hợp lệ: '" + l + "'");   // ProtocolError
            System.out.println("[client] S: " + l);
            text.add(l.length() > 4 ? l.substring(4) : "");             // dòng 6: rỗng nếu chỉ có 3 chữ số
            if (l.length() == 3 || l.charAt(3) == ' ') return new Reply(Integer.parseInt(l.substring(0, 3)), text);
        }
    }

    /** Mã giả 7.7, dòng 12–16: gửi lệnh, kiểm tra chữ số đầu của mã phản hồi. */
    static void cmd(Reader r, OutputStream out, String line, int expect, String shown) throws IOException {
        System.out.println("[client] C: " + (shown != null ? shown : line));
        out.write((line + "\r\n").getBytes(StandardCharsets.UTF_8));    // SEND_ALL
        out.flush();
        Reply rep = smtpReply(r);
        if (rep.code() / 100 != expect) throw new SmtpException(rep.code(), rep.text());
    }

    static String dotStuff(String msg) {                                // thêm '.' cho dòng bắt đầu bằng '.'
        return Arrays.stream(msg.split("\n", -1)).map(l -> l.startsWith(".") ? "." + l : l)
                .collect(Collectors.joining("\r\n"));
    }

    /** Mã giả 7.7, dòng 17–31. */
    static void sendMail(String host, int port, String caFile, String user, String secret,
                         String from, String to, String msg) throws Exception {
        try (Socket s = new Socket()) {
            s.connect(new InetSocketAddress(host, port), 5000);           // SAFE_CONNECT(host, 587, 5000)
            s.setSoTimeout(5000);
            Reader r = new Reader(s.getInputStream());
            OutputStream out = s.getOutputStream();
            Reply hello = smtpReply(r);                                 // 220: lời chào của server
            if (hello.code() != 220) throw new SmtpException(hello.code(), List.of("Không phải lời chào 220"));
            cmd(r, out, "EHLO client.example", 2, null);
            cmd(r, out, "STARTTLS", 2, null);                           // 220: sẵn sàng TLS
            SSLSocket tls = (SSLSocket) socketFactory(caFile).createSocket(s, host, port, true);   // TLS_WRAP
            SSLParameters p = tls.getSSLParameters();
            p.setEndpointIdentificationAlgorithm("HTTPS");               // bắt buộc kiểm tra tên máy trong chứng chỉ
            tls.setSSLParameters(p);
            tls.startHandshake();
            r = new Reader(tls.getInputStream());                       // Reader mới: bỏ mọi byte nhận trước TLS
            out = tls.getOutputStream();
            System.out.println("[client] Đã thiết lập " + tls.getSession().getProtocol()
                    + ", chứng chỉ server hợp lệ cho '" + host + "'");
            cmd(r, out, "EHLO client.example", 2, null);                // chào lại sau khi có TLS
            String token = Base64.getEncoder().encodeToString(("\0" + user + "\0" + secret).getBytes(StandardCharsets.UTF_8));
            cmd(r, out, "AUTH PLAIN " + token, 2, "AUTH PLAIN ****");    // không in thông tin xác thực
            cmd(r, out, "MAIL FROM:<" + from + ">", 2, null);
            cmd(r, out, "RCPT TO:<" + to + ">", 2, null);
            cmd(r, out, "DATA", 3, null);                               // 354: bắt đầu nội dung
            String body = dotStuff(msg);
            cmd(r, out, body + "\r\n.", 2, "<nội dung thư " + body.getBytes(StandardCharsets.UTF_8).length + " byte> + dòng '.'");
            cmd(r, out, "QUIT", 2, null);
            tls.close();
        }
    }

    /** Tin cậy chứng chỉ tự ký trong tệp CA (không tắt kiểm tra); "-" dùng kho chứng chỉ mặc định của JDK. */
    static SSLSocketFactory socketFactory(String caFile) throws Exception {
        if (caFile.equals("-")) return (SSLSocketFactory) SSLSocketFactory.getDefault();
        KeyStore ks = KeyStore.getInstance(KeyStore.getDefaultType());
        ks.load(null, null);
        try (InputStream in = new FileInputStream(caFile)) {
            ks.setCertificateEntry("ca", CertificateFactory.getInstance("X.509").generateCertificate(in));
        }
        TrustManagerFactory tmf = TrustManagerFactory.getInstance(TrustManagerFactory.getDefaultAlgorithm());
        tmf.init(ks);
        SSLContext ctx = SSLContext.getInstance("TLS");
        ctx.init(null, tmf.getTrustManagers(), null);
        return ctx.getSocketFactory();
    }

    /** Thư Internet tối giản: tiêu đề (Subject mã hóa MIME UTF-8), dòng trống, thân văn bản UTF-8. */
    static String buildMessage(String from, String to) {
        String subject = "=?utf-8?B?" + Base64.getEncoder().encodeToString("Thư thử nghiệm Bài 7.6".getBytes(StandardCharsets.UTF_8)) + "?=";
        String date = ZonedDateTime.now(ZoneOffset.UTC).format(DateTimeFormatter.ofPattern("EEE, dd MMM yyyy HH:mm:ss '+0000'", Locale.ENGLISH));
        return String.join("\n", "From: <" + from + ">", "To: <" + to + ">", "Subject: " + subject, "Date: " + date,
                "Message-ID: <" + System.currentTimeMillis() / 1000 + "@client.example>",
                "MIME-Version: 1.0", "Content-Type: text/plain; charset=utf-8", "Content-Transfer-Encoding: 8bit",
                "", "Xin chào, đây là thư gửi bằng phiên SMTP tự cài đặt.",
                ".Dòng này bắt đầu bằng dấu chấm (được DOT_STUFF thêm một dấu chấm).", "Hết thư.");
    }
}
