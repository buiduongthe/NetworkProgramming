import java.io.BufferedReader;
import java.io.FileDescriptor;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.PrintStream;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.KeyStore;
import java.security.cert.CertificateFactory;
import java.security.cert.X509Certificate;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.locks.ReentrantLock;
import javax.net.ssl.SSLContext;
import javax.net.ssl.SSLHandshakeException;
import javax.net.ssl.SSLParameters;
import javax.net.ssl.SSLSocket;
import javax.net.ssl.TrustManagerFactory;

/**
 * Bài 8.4 – Client chat tối giản có TLS và xác thực token (Mã giả 8.6 phía client; Mục 8.4.5).
 * CONNECT = SAFE_CONNECT rồi bọc TLS với tên máy cần kiểm tra; kế thừa Mã giả 5.5 (ChatClient: vòng nhận
 * chạy nền, khóa gửi). Tệp CA: nếu chỉ định thì tin cậy CA nội bộ đó, nếu không dùng kho tin cậy mặc định của JDK.
 * Chạy: java -cp out TlsChatClient <host> <port> <tệp_token> [tệp_CA]
 */
public class TlsChatClient {
    static final PrintStream OUT = new PrintStream(new FileOutputStream(FileDescriptor.out), true, StandardCharsets.UTF_8);
    SSLSocket tls;
    final ReentrantLock sendLock = new ReentrantLock();                    // mỗi lúc gửi một khung
    final CountDownLatch welcome = new CountDownLatch(1), pong = new CountDownLatch(1);
    volatile boolean ok, done;

    static void log(String t) { OUT.println("[client] " + t); }

    /** Mã giả 2.2 – SAFE_CONNECT (rút gọn): thử lần lượt các địa chỉ, chờ tối đa timeoutMs mỗi địa chỉ. */
    static Socket safeConnect(String host, int port, int timeoutMs) throws IOException {
        IOException last = null;
        for (InetAddress a : InetAddress.getAllByName(host)) {
            Socket s = new Socket();
            try { s.connect(new InetSocketAddress(a, port), timeoutMs); return s; }
            catch (IOException e) { last = e; s.close(); }
        }
        throw new IOException("Không kết nối được " + host + ":" + port + ": " + (last == null ? "" : last.getMessage()));
    }

    static SSLContext trustContext(String caFile) throws Exception {
        if (caFile == null) return SSLContext.getDefault();               // kho tin cậy mặc định
        X509Certificate ca;
        try (FileInputStream f = new FileInputStream(caFile)) {
            ca = (X509Certificate) CertificateFactory.getInstance("X.509").generateCertificate(f);
        }
        KeyStore ks = KeyStore.getInstance("PKCS12");
        ks.load(null, null);
        ks.setCertificateEntry("lab-ca", ca);                              // chỉ tin cậy CA nội bộ
        TrustManagerFactory tmf = TrustManagerFactory.getInstance(TrustManagerFactory.getDefaultAlgorithm());
        tmf.init(ks);
        SSLContext ctx = SSLContext.getInstance("TLS");
        ctx.init(null, tmf.getTrustManagers(), null);
        return ctx;
    }

    void connect(String host, int port, String token, SSLContext ctx) throws IOException {
        Socket raw = safeConnect(host, port, 3000);
        tls = (SSLSocket) ctx.getSocketFactory().createSocket(raw, host, port, true);   // TLS_WRAP(raw, CLIENT, host)
        SSLParameters p = tls.getSSLParameters();
        p.setEndpointIdentificationAlgorithm("HTTPS");                     // bắt buộc để so khớp tên máy với SAN
        p.setProtocols(new String[]{"TLSv1.3", "TLSv1.2"});
        tls.setSSLParameters(p);
        tls.startHandshake();
        X509Certificate peer = (X509Certificate) tls.getSession().getPeerCertificates()[0];
        log("Đã kết nối TLS tới " + host + ":" + port + ": " + tls.getSession().getProtocol() + ", " + tls.getSession().getCipherSuite());
        log("Chứng chỉ server " + peer.getSubjectX500Principal().getName() + " hợp lệ, tên '" + host + "' khớp SAN");
        send(Json.obj("type", "hello", "token", token));
        Thread.ofVirtual().start(this::receiveLoop);                       // RUN_BACKGROUND: không chờ
    }

    @SuppressWarnings("unchecked")
    void receiveLoop() {
        try {
            while (true) {
                Map<String, Object> m = Frame.recvJson(tls.getInputStream());
                switch (String.valueOf(m.get("type"))) {
                    case "welcome" -> {
                        ok = true;
                        log("Server chấp nhận: tên = " + m.get("name") + " (từ token), trực tuyến: "
                                + String.join(", ", (List<String>) (List<?>) m.get("users")));
                        welcome.countDown();
                    }
                    case "error" -> { log("Lỗi từ server: " + m.get("code") + " " + m.get("text")); welcome.countDown(); }
                    case "sys" -> log("* " + m.get("text"));
                    case "msg" -> log("[" + m.get("room") + "] " + m.get("from") + ": " + m.get("text"));
                    case "pong" -> pong.countDown();
                    default -> { }
                }
            }
        } catch (IOException e) {
            if (!done) log("Server đã đóng kết nối");
            welcome.countDown();
            pong.countDown();
        }
    }

    void send(Map<String, Object> obj) throws IOException {                 // SEND_MESSAGE: khóa gửi
        sendLock.lock();
        try { Frame.sendJson(tls.getOutputStream(), obj); } finally { sendLock.unlock(); }
    }

    public static void main(String[] args) throws Exception {
        if (args.length < 3) { log("Cách dùng: java -cp out TlsChatClient <host> <port> <tệp_token> [tệp_CA]"); System.exit(1); }
        String host = args[0], tokenFile = args[2], caFile = args.length > 3 ? args[3] : null;
        int port = Integer.parseInt(args[1]);
        String token = Files.readString(Path.of(tokenFile), StandardCharsets.US_ASCII).trim();
        log("Tin cậy: " + (caFile != null ? "CA trong " + caFile : "kho chứng chỉ mặc định của JDK") + "; token: " + tokenFile);
        TlsChatClient c = new TlsChatClient();
        try {
            c.connect(host, port, token, trustContext(caFile));
        } catch (SSLHandshakeException e) {
            log("Bắt tay TLS thất bại: " + e.getMessage());
            System.exit(1);
        } catch (IOException e) {
            log("Không kết nối được: " + e.getMessage());
            System.exit(1);
        }
        c.welcome.await(5, TimeUnit.SECONDS);
        if (!c.ok) { c.tls.close(); System.exit(1); }
        c.send(Json.obj("type", "join", "room", "chung"));
        BufferedReader in = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
        String line;
        while ((line = in.readLine()) != null)                             // READ_CONSOLE: các dòng cần gửi
            c.send(Json.obj("type", "msg", "room", "chung", "text", line));
        c.send(Json.obj("type", "ping"));                                  // pong về sau mọi tin trước đó
        c.pong.await(5, TimeUnit.SECONDS);
        c.done = true;
        c.tls.close();                                                     // DISCONNECT
        log("Đã ngắt kết nối");
    }
}
