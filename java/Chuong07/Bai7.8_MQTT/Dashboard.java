import java.io.IOException;

/**
 * Bài 7.8 – Bảng theo dõi đăng ký bộ lọc có ký tự đại diện (Mã giả 7.9, PROCEDURE DASHBOARD; Mục 7.5.5).
 * Chạy: java -cp out Dashboard [host] [port] [keep_alive_s] [client_id]   (mặc định localhost 1883 60 dashboard-01)
 *       Hai client trùng client_id thì broker ngắt client cũ, nên bảng thứ hai cần client_id khác.
 */
public class Dashboard {
    static volatile boolean stopping;                                  // đang dừng theo Ctrl+C

    public static void main(String[] a) throws IOException {
        String host = a.length >= 1 ? a[0] : "localhost";
        int port = a.length >= 2 ? Integer.parseInt(a[1]) : 1883;
        int keepAlive = a.length >= 3 ? Integer.parseInt(a[2]) : 60;
        String clientId = a.length >= 4 ? a[3] : "dashboard-01";
        MqttClient c = new MqttClient();
        c.connect(host, port, clientId, keepAlive, null);
        System.out.println("[dashboard] Đã kết nối broker " + host + ":" + port + " là '" + clientId
                + "' (MQTT 5.0, keep alive " + keepAlive + " s)");
        Thread main = Thread.currentThread();
        Runtime.getRuntime().addShutdownHook(new Thread(() -> {        // Ctrl+C: ngắt kết nối đúng thủ tục
            if (!main.isAlive()) return;
            stopping = true;
            try { c.disconnect(); System.out.println("[dashboard] Đã gửi DISCONNECT"); } catch (IOException ignored) { }
        }));
        int g1 = c.subscribe("nhamay/+/nhietdo", 0, Dashboard::onTemp);
        System.out.println("[dashboard] SUBSCRIBE nhamay/+/nhietdo (QoS 0) -> SUBACK, QoS được cấp " + g1);
        int g2 = c.subscribe("nhamay/+/trangthai", 1, Dashboard::onStatus);
        System.out.println("[dashboard] SUBSCRIBE nhamay/+/trangthai (QoS 1) -> SUBACK, QoS được cấp " + g2);
        try {
            c.runEventLoop(() -> System.out.println("[dashboard] PINGREQ -> PINGRESP"));
        } catch (IOException e) {
            if (!stopping) System.out.println("[dashboard] " + e.getMessage());
        }
    }

    static void onTemp(String topic, String payload, int qos, boolean retain) {
        System.out.println("[dashboard] Nhiệt độ  " + topic + ": " + payload);
    }

    static void onStatus(String topic, String payload, int qos, boolean retain) {
        System.out.println("[dashboard] Trạng thái " + topic + ": " + payload + " (QoS " + qos + (retain ? ", tin giữ lại" : "") + ")");
    }
}
