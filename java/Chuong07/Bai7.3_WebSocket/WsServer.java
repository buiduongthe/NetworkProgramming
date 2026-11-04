import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.Base64;
import java.util.Map;

/**
 * Bài 7.3 – Server WebSocket tối giản (Mã giả 7.3 WS_ACCEPT, Mã giả 7.4 WS_READ_FRAME; Mục 7.3.2).
 * Kế thừa: Mã giả 7.2 (dòng đầu, READ_HEADERS, RESPOND), 3.4 (READ_EXACT), 6.3 (luồng ảo cho mỗi kết nối).
 * Biên dịch: javac -encoding UTF-8 -d out *.java
 * Chạy:      java -cp out WsServer [host] [port]      (mặc định 0.0.0.0 8080)
 */
public class WsServer {
    static final String WS_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
    static final long MAX_FRAME = 1048576;                              // 1 MiB (Mã giả 3.6)

    static class ProtocolException extends IOException {
        ProtocolException(String m) { super(m); }
    }

    record Frame(boolean fin, int opcode, byte[] data) {}

    public static void main(String[] args) throws IOException {
        String host = args.length >= 1 ? args[0] : "0.0.0.0";
        int port = args.length >= 2 ? Integer.parseInt(args[1]) : 8080;
        try (ServerSocket srv = new ServerSocket()) {
            srv.setReuseAddress(true);
            srv.bind(new InetSocketAddress(host, port), 16);
            System.out.println("[server] Đang lắng nghe tại " + host + ":" + port);
            while (true) {
                Socket conn = srv.accept();
                Thread.startVirtualThread(() -> session(conn));         // RUN_BACKGROUND trên luồng ảo
            }
        }
    }

    /** Mã giả 7.3: kiểm tra yêu cầu nâng cấp và gửi 101 kèm Sec-WebSocket-Accept. */
    /** BASE64_DECODE; chuỗi không phải Base64 hợp lệ cho kết quả rỗng (sẽ bị từ chối). */
    static byte[] base64Decode(String text) {
        try { return Base64.getDecoder().decode(text); }
        catch (IllegalArgumentException e) { return new byte[0]; }
    }

    static boolean wsAccept(OutputStream out, Map<String, String> hdr) throws IOException {
        String key = hdr.getOrDefault("sec-websocket-key", "");         // dòng 3
        if (!hdr.getOrDefault("upgrade", "").equalsIgnoreCase("websocket")   // dòng 4–6
                || !"13".equals(hdr.get("sec-websocket-version"))
                || base64Decode(key).length != 16) {                    // khóa: đúng 16 byte
            byte[] b = "{\"type\":\"about:blank\",\"title\":\"Sai WebSocket\",\"status\":400}".getBytes(StandardCharsets.UTF_8);
            out.write(("HTTP/1.1 400 Bad Request\r\nContent-Type: application/problem+json; charset=utf-8\r\n"
                    + "Content-Length: " + b.length + "\r\n\r\n").getBytes(StandardCharsets.US_ASCII));
            out.write(b);
            out.flush();
            return false;
        }
        String acc = Base64.getEncoder().encodeToString(sha1(key + WS_GUID));
        String h = "HTTP/1.1 101 Switching Protocols\r\n" + "Upgrade: websocket\r\nConnection: Upgrade\r\n"
                + "Sec-WebSocket-Accept: " + acc + "\r\n\r\n";
        out.write(h.getBytes(StandardCharsets.US_ASCII));
        out.flush();
        return true;                                                    // từ đây trao đổi bằng khung WebSocket
    }

    static byte[] sha1(String s) {
        try {
            return MessageDigest.getInstance("SHA-1").digest(s.getBytes(StandardCharsets.US_ASCII));
        } catch (NoSuchAlgorithmException e) { throw new IllegalStateException(e); }
    }

    /** Gửi một khung từ server: FIN = 1, không che. */
    static void wsSend(OutputStream out, int opcode, byte[] data) throws IOException {
        ByteBuffer head = ByteBuffer.allocate(10);                      // ByteBuffer mặc định big-endian
        head.put((byte) (0x80 | opcode));
        if (data.length < 126) head.put((byte) data.length);
        else if (data.length < 65536) head.put((byte) 126).putShort((short) data.length);
        else head.put((byte) 127).putLong(data.length);
        out.write(head.array(), 0, head.position());
        out.write(data);
        out.flush();
    }

    /** WS_CLOSE(s, code): khung opcode 0x8 chứa mã đóng 2 byte. */
    static void wsClose(OutputStream out, int code) throws IOException {
        wsSend(out, 0x8, new byte[] {(byte) (code >> 8), (byte) code});
    }

    /** Mã giả 7.4: đọc một khung WebSocket. */
    static Frame wsReadFrame(Reader r, OutputStream out, boolean fromClient) throws IOException {
        byte[] b = r.readExact(2);
        boolean fin = (b[0] & 0x80) != 0; int opcode = b[0] & 0x0F;
        boolean masked = (b[1] & 0x80) != 0; long n = b[1] & 0x7F;
        if (n == 126) n = ByteBuffer.wrap(r.readExact(2)).getShort() & 0xFFFF;
        if (n == 127) n = ByteBuffer.wrap(r.readExact(8)).getLong();
        if (n > MAX_FRAME || n < 0) {                                   // n < 0: bit cao = 1 (long có dấu)
            wsClose(out, 1009);                                         // 1009: thông điệp quá lớn
            throw new ProtocolException("Khung quá lớn");
        }
        if (fromClient && !masked) {
            wsClose(out, 1002);                                         // 1002: vi phạm giao thức
            throw new ProtocolException("Khung client phải được che");
        }
        byte[] mask = masked ? r.readExact(4) : null;
        byte[] data = r.readExact((int) n);
        if (masked) for (int i = 0; i < data.length; i++) data[i] ^= mask[i % 4];   // XOR, khóa lặp theo vòng
        return new Frame(fin, opcode, data);
    }

    static void session(Socket conn) {
        String addr = conn.getInetAddress().getHostAddress() + ":" + conn.getPort();
        try (conn) {
            Reader r = new Reader(conn.getInputStream());
            OutputStream out = conn.getOutputStream();
            String line = r.readLine(8192);
            if (line == null) return;
            if (!wsAccept(out, r.readHeaders())) {
                System.out.println("[server] " + addr + " " + line + " -> 400 (không phải yêu cầu WebSocket)");
                return;
            }
            System.out.println("[server] Bắt tay WebSocket thành công với " + addr);
            ByteArrayOutputStream message = new ByteArrayOutputStream(); int msgType = 0;
            while (true) {
                Frame f = wsReadFrame(r, out, true);
                if (f.opcode() == 0x8) {                                // đóng: gửi lại mã đóng
                    int code = f.data().length >= 2 ? ((f.data()[0] & 0xFF) << 8) | (f.data()[1] & 0xFF) : 1005;
                    System.out.println("[server] " + addr + " gửi khung đóng, mã " + code);
                    wsClose(out, code == 1005 ? 1000 : code);
                    return;
                }
                if (f.opcode() == 0x9) {                                // ping -> pong cùng dữ liệu
                    System.out.println("[server] Nhận ping: '" + new String(f.data(), StandardCharsets.UTF_8) + "' -> gửi pong");
                    wsSend(out, 0xA, f.data());
                    continue;
                }
                if (f.opcode() == 0x1 || f.opcode() == 0x2) { message.reset(); msgType = f.opcode(); }
                message.write(f.data());                                // opcode 0x0: khung tiếp nối
                if (!f.fin()) continue;
                byte[] msg = message.toByteArray();
                if (msgType == 0x1) {
                    String text = new String(msg, StandardCharsets.UTF_8);
                    System.out.println("[server] Nhận văn bản (" + msg.length + " byte): '"
                            + text.substring(0, Math.min(40, text.length())) + "'");
                    wsSend(out, 0x1, ("echo: " + text).getBytes(StandardCharsets.UTF_8));
                } else {
                    System.out.println("[server] Nhận nhị phân (" + msg.length + " byte)");
                    wsSend(out, 0x2, msg);
                }
            }
        } catch (ProtocolException e) {
            System.out.println("[server] Lỗi giao thức với " + addr + ": " + e.getMessage());
        } catch (IOException e) {
            System.out.println("[server] Kết nối " + addr + " đóng đột ngột: " + e.getMessage());
        } finally {
            System.out.println("[server] Đóng kết nối " + addr);
        }
    }
}
