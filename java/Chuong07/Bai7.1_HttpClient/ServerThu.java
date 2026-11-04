import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;

/**
 * Bài 7.1 – Server HTTP thử nghiệm cho client tối giản (khung server tuần tự của Mã giả 3.1).
 * Ba kiểu định khung thân: /api/v1/terms/TCP -> Content-Length; /chunked -> chunked (có mở rộng khối,
 * trailer); /dong-ket-noi -> không có độ dài, đóng kết nối; đường dẫn khác -> 404.
 * Chạy: java -cp out ServerThu [host] [port]      (mặc định 0.0.0.0 8080)
 */
public class ServerThu {
    public static void main(String[] args) throws IOException {
        String host = args.length >= 1 ? args[0] : "0.0.0.0";
        int port = args.length >= 2 ? Integer.parseInt(args[1]) : 8080;
        try (ServerSocket srv = new ServerSocket()) {
            srv.setReuseAddress(true);
            srv.bind(new InetSocketAddress(host, port), 16);
            System.out.println("[server] Đang lắng nghe tại " + host + ":" + port);
            while (true) {
                try (Socket conn = srv.accept()) {
                    Reader r = new Reader(conn.getInputStream());
                    String line = r.readLine(8192);                         // dòng yêu cầu
                    if (line == null) continue;
                    String h;
                    while ((h = r.readLine(8192)) != null && !h.isEmpty()) { }  // bỏ qua tiêu đề
                    String how = respond(conn.getOutputStream(), line.split(" ")[1]);
                    System.out.println("[server] " + line + " -> định khung: " + how);
                }
            }
        }
    }

    static String respond(OutputStream out, String path) throws IOException {
        if (path.equals("/api/v1/terms/TCP")) {
            byte[] body = "{\"term\":\"TCP\",\"text\":\"Giao thức vận chuyển hướng kết nối\"}".getBytes(StandardCharsets.UTF_8);
            send(out, "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\n"
                    + "Content-Length: " + body.length + "\r\n\r\n", body);
            return "Content-Length";
        }
        if (path.equals("/chunked")) {
            ByteArrayOutputStream b = new ByteArrayOutputStream();
            String[] parts = {"Thân phản hồi ", "được chia thành ", "nhiều khối."};
            for (int i = 0; i < parts.length; i++) {
                byte[] p = parts[i].getBytes(StandardCharsets.UTF_8);
                String ext = i == 0 ? ";vi-du=mo-rong" : "";                // phần mở rộng khối
                b.write((Integer.toHexString(p.length).toUpperCase() + ext + "\r\n").getBytes(StandardCharsets.US_ASCII));
                b.write(p);
                b.write("\r\n".getBytes(StandardCharsets.US_ASCII));
            }
            b.write("0\r\nX-So-Khoi: 3\r\n\r\n".getBytes(StandardCharsets.US_ASCII));   // khối 0, trailer
            send(out, "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\n"
                    + "Transfer-Encoding: chunked\r\nTrailer: X-So-Khoi\r\n\r\n", b.toByteArray());
            return "chunked";
        }
        if (path.equals("/dong-ket-noi")) {
            send(out, "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n",
                    "Không có Content-Length: client đọc đến khi kết nối đóng.".getBytes(StandardCharsets.UTF_8));
            return "đóng kết nối";
        }
        byte[] nf = "{\"type\":\"about:blank\",\"title\":\"Not Found\",\"status\":404}".getBytes(StandardCharsets.UTF_8);
        send(out, "HTTP/1.1 404 Not Found\r\nContent-Type: application/problem+json\r\n"
                + "Content-Length: " + nf.length + "\r\n\r\n", nf);
        return "Content-Length";
    }

    static void send(OutputStream out, String head, byte[] body) throws IOException {
        out.write(head.getBytes(StandardCharsets.US_ASCII));
        out.write(body);
        out.flush();
    }
}
