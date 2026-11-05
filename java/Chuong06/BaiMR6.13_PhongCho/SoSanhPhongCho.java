import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.function.Consumer;

/**
 * Bài mở rộng MR6.13 – So sánh BẬT / TẮT phòng chờ ảo (dòng lệnh).
 *
 * Chạy hai lượt mô phỏng ở cùng một chế độ (mặc định khoa, đúng): phòng chờ TẮT (mọi người dồn vào "mua", máy chủ giới
 * hạn số yêu cầu đang xử lý -> từ chối "qua_tai", người dùng bấm lại) rồi BẬT (xếp hàng, cấp lượt theo thùng token).
 * Bảng: số đơn mua được, số lần bị từ chối vì quá tải, số người hết vé, thời gian mua p50/p99 (từ lần bấm "mua" đầu
 * tiên tới kết quả cuối, gồm cả các lần bấm lại) và thời gian cả lượt.
 *
 * Chạy:  java -cp out SoSanhPhongCho [host] [cổng] [--n 1000] [--k 200] [--che-do khoa]
 */
public class SoSanhPhongCho {
    public static void main(String[] args) throws IOException {
        List<String> vt = new ArrayList<>();
        int n = 1000;
        Integer k = null;
        String cheDo = "khoa";
        for (int i = 0; i < args.length; i++) {
            switch (args[i]) {
                case "--n" -> n = Integer.parseInt(args[++i]);
                case "--k" -> k = Integer.parseInt(args[++i]);
                case "--che-do" -> {
                    cheDo = args[++i];
                    if (!Chung.CHE_DO.contains(cheDo)) throw new IllegalArgumentException("--che-do: " + Chung.CHE_DO);
                }
                default -> vt.add(args[i]);
            }
        }
        String host = vt.size() > 0 ? vt.get(0) : "127.0.0.1";
        int cong = vt.size() > 1 ? Integer.parseInt(vt.get(1)) : 43770;
        int kk = k != null && k > 0 ? k : Math.min(n, 200);
        Consumer<String> bao = text -> Chung.inDong("mo_phong", text);
        List<Map<String, Object>> hang = MoPhong.soSanhPhongCho(host, cong, n, kk, bao, cheDo);
        bao.accept("Bật/tắt phòng chờ (" + n + " người, " + cheDo + ", " + Math.min(kk, n) + " kết nối):");
        for (String d : MoPhong.bangPhongCho(hang)) Chung.inTho(d);
        System.exit(0);
    }
}
