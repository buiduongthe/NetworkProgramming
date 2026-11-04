import java.io.DataInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.SecureRandom;
import java.util.Arrays;
import java.util.Base64;

/**
 * Bài 7.3 – Kiểm thử server bằng khung tự tạo (java.net.http.WebSocket không cho gửi khung sai đặc tả):
 * 1. HTTP thường -> 400; 2. ping -> pong; 3. văn bản 300 byte (độ dài 16 bit); 4. khung không che -> 1002;
 * 5. khung khai báo 2^40 byte -> 1009; 6. Sec-WebSocket-Key không phải 16 byte -> 400.
 * Chạy: java -cp out WsKiemThu [host] [port]      (mặc định 127.0.0.1 8080)
 */
public class WsKiemThu {
    static final SecureRandom RNG = new SecureRandom();

    public static void main(String[] args) throws Exception {
        String host = args.length >= 1 ? args[0] : "127.0.0.1";
        int port = args.length >= 2 ? Integer.parseInt(args[1]) : 8080;
        try (Socket s = new Socket(host, port)) {                      // ca 1
            s.getOutputStream().write(("GET / HTTP/1.1\r\nHost: " + host + ":" + port + "\r\n\r\n").getBytes(StandardCharsets.US_ASCII));
            byte[] buf = new byte[4096]; int n = s.getInputStream().read(buf);
            System.out.println("[kiểm thử] 1. Yêu cầu HTTP thường (không Upgrade) -> "
                    + new String(buf, 0, n, StandardCharsets.US_ASCII).split("\r\n")[0]);
        }
        try (Socket s = connect(host, port)) {                         // ca 2
            send(s, 0x9, "nhip-tim".getBytes(StandardCharsets.US_ASCII), true);
            byte[][] f = readFrame(s);
            System.out.printf("[kiểm thử] 2. Ping 'nhip-tim' -> opcode 0x%X, dữ liệu '%s'%n", f[0][0], new String(f[1], StandardCharsets.US_ASCII));
            send(s, 0x8, new byte[] {0x03, (byte) 0xE8}, true); readFrame(s);
        }
        try (Socket s = connect(host, port)) {                         // ca 3
            byte[] a = new byte[300]; Arrays.fill(a, (byte) 'A');
            send(s, 0x1, a, true);
            byte[][] f = readFrame(s);
            System.out.printf("[kiểm thử] 3. Văn bản 300 byte -> nhận lại %d byte (opcode 0x%X)%n", f[1].length, f[0][0]);
            send(s, 0x8, new byte[] {0x03, (byte) 0xE8}, true); readFrame(s);
        }
        try (Socket s = connect(host, port)) {                         // ca 4
            send(s, 0x1, "khong che".getBytes(StandardCharsets.US_ASCII), false);
            byte[] d = readFrame(s)[1];
            System.out.println("[kiểm thử] 4. Khung không che -> khung đóng, mã " + (((d[0] & 0xFF) << 8) | (d[1] & 0xFF)));
        }
        try (Socket s = connect(host, port)) {                         // ca 5: chỉ gửi tiêu đề khung
            s.getOutputStream().write(new byte[] {(byte) 0x81, (byte) (0x80 | 127), 0, 0, 0x01, 0, 0, 0, 0, 0});  // 2^40
            byte[] d = readFrame(s)[1];
            System.out.println("[kiểm thử] 5. Khung khai báo 2^40 byte -> khung đóng, mã " + (((d[0] & 0xFF) << 8) | (d[1] & 0xFF)));
        }
        try (Socket s = new Socket(host, port)) {                      // ca 6: khóa không phải 16 byte
            s.getOutputStream().write(("GET / HTTP/1.1\r\nHost: " + host + ":" + port + "\r\nUpgrade: websocket\r\n"
                    + "Connection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: c2hvcnQ=\r\n\r\n")
                    .getBytes(StandardCharsets.US_ASCII));
            byte[] buf = new byte[4096]; int n = s.getInputStream().read(buf);
            System.out.println("[kiểm thử] 6. Sec-WebSocket-Key 'c2hvcnQ=' (5 byte) -> "
                    + new String(buf, 0, n, StandardCharsets.US_ASCII).split("\r\n")[0]);
        }
    }

    /** Gửi yêu cầu nâng cấp và kiểm tra Sec-WebSocket-Accept (phía client của Mã giả 7.3). */
    static Socket connect(String host, int port) throws Exception {
        Socket s = new Socket(host, port);
        s.setSoTimeout(5000);
        byte[] k = new byte[16]; RNG.nextBytes(k);
        String key = Base64.getEncoder().encodeToString(k);
        s.getOutputStream().write(("GET / HTTP/1.1\r\nHost: " + host + ":" + port + "\r\nUpgrade: websocket\r\n"
                + "Connection: Upgrade\r\nSec-WebSocket-Key: " + key + "\r\nSec-WebSocket-Version: 13\r\n\r\n").getBytes(StandardCharsets.US_ASCII));
        StringBuilder head = new StringBuilder();
        InputStream in = s.getInputStream();
        while (!head.toString().endsWith("\r\n\r\n")) head.append((char) in.read());   // đọc từng byte
        String expect = Base64.getEncoder().encodeToString(MessageDigest.getInstance("SHA-1")
                .digest((key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").getBytes(StandardCharsets.US_ASCII)));
        if (!head.toString().contains("Sec-WebSocket-Accept: " + expect)) throw new IOException("Bắt tay thất bại");
        return s;
    }

    static void send(Socket s, int opcode, byte[] data, boolean mask) throws IOException {
        OutputStream out = s.getOutputStream();
        int m = mask ? 0x80 : 0;
        out.write(0x80 | opcode);
        if (data.length < 126) out.write(m | data.length);
        else { out.write(m | 126); out.write(data.length >> 8); out.write(data.length & 0xFF); }
        byte[] payload = data.clone();
        if (mask) {
            byte[] key = new byte[4]; RNG.nextBytes(key);
            out.write(key);
            for (int i = 0; i < payload.length; i++) payload[i] ^= key[i % 4];
        }
        out.write(payload);
        out.flush();
    }

    /** Mã giả 7.4 với from_client = FALSE (độ dài 7 hoặc 16 bit đủ cho ca kiểm thử); trả {opcode}, dữ liệu. */
    static byte[][] readFrame(Socket s) throws IOException {
        DataInputStream in = new DataInputStream(s.getInputStream());
        int b0 = in.readUnsignedByte(), n = in.readUnsignedByte() & 0x7F;
        if (n == 126) n = in.readUnsignedShort();
        byte[] data = new byte[n];
        in.readFully(data);                                             // RECV_EXACT
        return new byte[][] {{(byte) (b0 & 0x0F)}, data};
    }
}
