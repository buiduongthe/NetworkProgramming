import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/**
 * Bài mở rộng MR6.11 – Dòng lệnh quản trị (giống nhau ở các bài tranh chấp).
 *
 * Chạy:  java -cp out Khach <host> <cổng> che_do <x> | tre <ms> | dat_lai k=v … | thong_ke
 */
public class Khach {
    static void bao(String text) { Chung.inDong("quan_tri", text); }

    public static void main(String[] args) {
        if (args.length < 3) {
            Chung.inTho("Dùng: Khach <host> <cổng> che_do <x> | tre <ms> | dat_lai k=v … | thong_ke");
            return;
        }
        String host = args[0], lenh = args[2];
        int cong = Integer.parseInt(args[1]);
        List<String> ts = Arrays.asList(args).subList(3, args.length);
        QuanTri qt;
        try {
            qt = new QuanTri(host, cong, "quan_tri");
        } catch (IOException e) {
            bao("Không kết nối được: " + e.getMessage());
            System.exit(1);
            return;
        }
        try {
            List<String> tat = new ArrayList<>(List.of(lenh));
            tat.addAll(ts);
            bao("> " + String.join(" ", tat));
            if (lenh.equals("che_do") && ts.size() == 1) qt.cheDo(ts.get(0));
            else if (lenh.equals("tre") && ts.size() == 1) qt.tre(Double.parseDouble(ts.get(0)));
            else if (lenh.equals("dat_lai")) qt.datLai(Chung.docThamSo(ts));
            else if (!lenh.equals("thong_ke")) {
                bao("lệnh không hợp lệ");
                return;
            }
            bao("OK");
            if (lenh.equals("thong_ke")) for (String d : Chung.dongThongKe(qt.thongKe())) bao(d);
        } catch (IllegalArgumentException e) {
            bao("Bị từ chối: " + e.getMessage());
        } catch (IOException e) {
            bao("lỗi: " + e.getMessage());
        } finally {
            qt.dong();
        }
    }
}
