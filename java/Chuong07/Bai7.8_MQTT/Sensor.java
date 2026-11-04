import java.util.Arrays;
import java.util.Locale;
import java.util.concurrent.ThreadLocalRandom;

/**
 * Bài 7.8 – Thiết bị cảm biến phát số đo qua MQTT (Mã giả 7.9, PROCEDURE SENSOR; Mục 7.5.5).
 * Biên dịch: javac -encoding UTF-8 -d out *.java
 * Chạy: java -cp out Sensor <mã_thiết_bị> [host] [port] [số_lần] [chu_kỳ_ms] [--mat-ket-noi]
 *       (mặc định localhost 1883, số_lần 0 = chạy mãi, chu kỳ 5000 ms)
 */
public class Sensor {
    public static void main(String[] all) throws Exception {
        boolean lose = Arrays.asList(all).contains("--mat-ket-noi");
        String[] a = Arrays.stream(all).filter(x -> !x.equals("--mat-ket-noi")).toArray(String[]::new);
        String deviceId = a.length >= 1 ? a[0] : "day1";
        String host = a.length >= 2 ? a[1] : "localhost";
        int port = a.length >= 3 ? Integer.parseInt(a[2]) : 1883;
        int times = a.length >= 4 ? Integer.parseInt(a[3]) : 0;
        int periodMs = a.length >= 5 ? Integer.parseInt(a[4]) : 5000;
        String st = "nhamay/" + deviceId + "/trangthai";
        MqttClient c = new MqttClient();
        c.connect(host, port, deviceId, 30, new MqttClient.Will(st, "offline", 1, true));
        System.out.println("[" + deviceId + "] Đã kết nối broker " + host + ":" + port
                + " (MQTT 5.0, keep alive 30 s, di chúc '" + st + "' = offline)");
        c.publish(st, "online", 1, true);
        System.out.println("[" + deviceId + "] PUBLISH " + st + " = online (QoS 1, giữ lại) -> đã nhận PUBACK");
        for (int n = 1; times == 0 || n <= times; n++) {
            double v = ThreadLocalRandom.current().nextDouble(24.0, 32.0);   // READ_SENSOR(): giá trị mô phỏng
            String msg = String.format(Locale.ROOT, "{\"v\":%.1f,\"ts\":%d}", v, System.currentTimeMillis() / 1000);
            c.publish("nhamay/" + deviceId + "/nhietdo", msg, 0, false);
            System.out.println("[" + deviceId + "] PUBLISH nhamay/" + deviceId + "/nhietdo = " + msg + " (QoS 0)");
            Thread.sleep(periodMs);
        }
        if (lose) {
            c.closeAbruptly();                                          // mô phỏng mất điện
            System.out.println("[" + deviceId + "] Mô phỏng mất kết nối: đóng socket, không gửi DISCONNECT");
        } else {
            c.publish(st, "offline", 1, true);
            c.disconnect();
            System.out.println("[" + deviceId + "] Tắt đúng thủ tục: phát offline rồi gửi DISCONNECT");
        }
    }
}
