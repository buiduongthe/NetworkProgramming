import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.net.SocketTimeoutException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/**
 * Bài 7.8 – Client MQTT 5.0 tối giản trên socket TCP (cài đặt các lệnh mức thư viện của Mã giả 7.9):
 * CONNECT (kèm di chúc) / CONNACK, PUBLISH QoS 0 và 1 (PUBACK), SUBSCRIBE / SUBACK, PINGREQ / PINGRESP, DISCONNECT.
 * Không hỗ trợ QoS 2, TLS, thuộc tính MQTT 5.0 (luôn gửi độ dài thuộc tính 0).
 * Định khung: 1 byte loại gói + cờ, "độ dài còn lại" varint 1–4 byte, rồi đúng chừng ấy byte (RECV_EXACT).
 */
final class MqttClient {
    interface Handler { void on(String topic, String payload, int qos, boolean retain); }
    record Packet(int type, int flags, byte[] body) {}
    record Will(String topic, String msg, int qos, boolean retain) {}
    record Sub(String filter, Handler fn) {}

    static final int CONNECT = 1, CONNACK = 2, PUBLISH = 3, PUBACK = 4, SUBSCRIBE = 8, SUBACK = 9,
            PINGREQ = 12, PINGRESP = 13, DISCONNECT = 14;
    private Socket sock;
    private InputStream in;
    private OutputStream out;
    private int nextId = 1, keepAlive;
    private long lastSend;
    private final List<Sub> handlers = new ArrayList<>();

    static byte[] str(String s) {                                       // chuỗi UTF-8: 2 byte độ dài + dữ liệu
        byte[] b = s.getBytes(StandardCharsets.UTF_8);
        return concat(new byte[] {(byte) (b.length >> 8), (byte) b.length}, b);
    }

    static byte[] varint(int n) {                                       // 7 bit mỗi byte, bit cao = còn byte tiếp
        ByteArrayOutputStream o = new ByteArrayOutputStream();
        do { int d = n & 0x7F; n >>>= 7; o.write(d | (n > 0 ? 0x80 : 0)); } while (n > 0);
        return o.toByteArray();
    }

    static byte[] concat(byte[]... parts) {
        ByteArrayOutputStream o = new ByteArrayOutputStream();
        for (byte[] p : parts) o.writeBytes(p);
        return o.toByteArray();
    }

    static boolean topicMatches(String flt, String topic) {             // '+' một cấp, '#' mọi cấp còn lại
        String[] f = flt.split("/", -1), t = topic.split("/", -1);
        for (int i = 0; i < f.length; i++) {
            if (f[i].equals("#")) return true;
            if (i >= t.length || (!f[i].equals("+") && !f[i].equals(t[i]))) return false;
        }
        return f.length == t.length;
    }

    private synchronized void send(int type, int flags, byte[] body) throws IOException {
        out.write(concat(new byte[] {(byte) ((type << 4) | flags)}, varint(body.length), body));   // SEND_ALL
        out.flush();
        lastSend = System.nanoTime();                                   // keep alive tính theo gói client GỬI
    }

    private byte[] recvExact(int n) throws IOException {                // Mã giả 3.4 – RECV_EXACT
        byte[] b = in.readNBytes(n);
        if (b.length < n) throw new IOException("Broker đóng kết nối");
        return b;
    }

    /** Đọc một gói; chỉ chờ byte đầu với firstTimeoutMs (hết giờ ở đây chưa mất byte nào của gói). */
    private Packet readPacket(int firstTimeoutMs) throws IOException {
        sock.setSoTimeout(firstTimeoutMs);
        int b0 = recvExact(1)[0] & 0xFF;
        sock.setSoTimeout(5000);
        int n = 0, shift = 0, d;
        do { d = recvExact(1)[0] & 0xFF; n |= (d & 0x7F) << shift; shift += 7; } while ((d & 0x80) != 0);
        return new Packet(b0 >> 4, b0 & 0x0F, recvExact(n));
    }

    private void dispatch(int flags, byte[] body) throws IOException {  // gói PUBLISH đến
        int qos = (flags >> 1) & 3; boolean retain = (flags & 1) != 0;
        int tlen = ((body[0] & 0xFF) << 8) | (body[1] & 0xFF), i = 2 + tlen;
        String topic = new String(body, 2, tlen, StandardCharsets.UTF_8);
        if (qos > 0) { send(PUBACK, 0, Arrays.copyOfRange(body, i, i + 2)); i += 2; }   // QoS 1: xác nhận
        int plen = 0, shift = 0, d;
        do { d = body[i++] & 0xFF; plen |= (d & 0x7F) << shift; shift += 7; } while ((d & 0x80) != 0);
        String payload = new String(body, i + plen, body.length - i - plen, StandardCharsets.UTF_8);   // bỏ thuộc tính
        for (Sub s : handlers) if (topicMatches(s.filter(), topic)) s.fn().on(topic, payload, qos, retain);
    }

    private byte[] waitFor(int type) throws IOException {               // PUBLISH đến giữa chừng vẫn được xử lý
        while (true) {
            Packet p = readPacket(5000);
            if (p.type() == type) return p.body();
            if (p.type() == PUBLISH) dispatch(p.flags(), p.body());
        }
    }

    /** MQTT_CONNECT; will có thể null. */
    void connect(String host, int port, String clientId, int keepAliveS, Will will) throws IOException {
        sock = new Socket();
        sock.connect(new InetSocketAddress(host, port), 5000);
        in = sock.getInputStream(); out = sock.getOutputStream(); keepAlive = keepAliveS;
        int flags = 0x02;                                               // Clean Start
        byte[] payload = str(clientId);
        if (will != null) {
            flags |= 0x04 | (will.qos() << 3) | (will.retain() ? 0x20 : 0);
            payload = concat(payload, new byte[] {0}, str(will.topic()), str(will.msg()));   // thuộc tính (0), chủ đề, nội dung
        }
        send(CONNECT, 0, concat(str("MQTT"), new byte[] {5, (byte) flags, (byte) (keepAliveS >> 8), (byte) keepAliveS, 0}, payload));
        byte[] ack = waitFor(CONNACK);
        if (ack[1] != 0) throw new IOException(String.format("CONNACK mã lý do 0x%02X", ack[1]));   // 0x00: thành công
    }

    void publish(String topic, String payload, int qos, boolean retain) throws IOException {
        byte[] body = str(topic);
        if (qos > 0) { int pid = nextId++; body = concat(body, new byte[] {(byte) (pid >> 8), (byte) pid}); }
        send(PUBLISH, (qos << 1) | (retain ? 1 : 0), concat(body, new byte[] {0}, payload.getBytes(StandardCharsets.UTF_8)));
        if (qos == 1) waitFor(PUBACK);                                  // QoS 1: chờ broker xác nhận
    }

    int subscribe(String flt, int qos, Handler fn) throws IOException {
        handlers.add(new Sub(flt, fn));                                 // đăng ký trước: tin giữ lại có thể đến ngay
        int pid = nextId++;
        send(SUBSCRIBE, 0x2, concat(new byte[] {(byte) (pid >> 8), (byte) pid, 0}, str(flt), new byte[] {(byte) qos}));
        byte[] ack = waitFor(SUBACK);
        int code = ack[3 + ack[2]] & 0xFF;                              // sau mã gói và thuộc tính
        if (code >= 0x80) throw new IOException(String.format("SUBACK từ chối %s: 0x%02X", flt, code));
        return code;
    }

    /** RUN_EVENT_LOOP: nhận gói, gọi hàm xử lý; chưa gửi gì trong keep_alive/2 giây thì gửi PINGREQ. */
    void runEventLoop(Runnable onPing) throws IOException {
        while (true) {
            int wait = keepAlive * 500 - (int) ((System.nanoTime() - lastSend) / 1_000_000);
            if (wait <= 0) { send(PINGREQ, 0, new byte[0]); continue; }
            Packet p;
            try { p = readPacket(wait); } catch (SocketTimeoutException e) { continue; }
            if (p.type() == PUBLISH) dispatch(p.flags(), p.body());
            else if (p.type() == PINGRESP && onPing != null) onPing.run();
        }
    }

    void disconnect() throws IOException {
        send(DISCONNECT, 0, new byte[0]);                               // mã lý do 0: broker bỏ di chúc
        sock.close();
    }

    void closeAbruptly() throws IOException { sock.close(); }           // đóng socket, không gửi DISCONNECT
}
