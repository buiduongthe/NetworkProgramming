import java.awt.Color;
import java.awt.Dimension;
import java.awt.GridBagLayout;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import javax.swing.BorderFactory;
import javax.swing.Box;
import javax.swing.JComponent;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.JTextField;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.14 – Bảng điều khiển máy chủ đặt vé xe khách theo chặng (Swing).
 *
 * Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký)
 * ở GiaoDienChung.CuaSoServerCoSo; tệp này chỉ vẽ khung "Tài nguyên": sơ đồ xe thu nhỏ (mỗi chỗ có dải vạch theo 3 chặng,
 * ghế nóng viền đỏ), số chỗ-chặng mỗi loại, số chỗ đã bán theo chặng và tham số đặt lại (thời hạn giữ chỗ, loại xe
 * giuong/ghe, ghế nóng của bộ mô phỏng).
 *
 * Chạy:  java -cp out ServerGui [host] [cổng]        (mặc định 127.0.0.1 43782; giữ chỗ 30 s, xe giường nằm)
 */
public class ServerGui extends GiaoDienChung.CuaSoServerCoSo {
    volatile DatVeXeGui.SoDoXe soDo;
    volatile Map<String, Object> ttVe;
    JComponent cv;
    final Map<String, JLabel> nhanDem = new LinkedHashMap<>();
    JLabel nhanXe, nhanChang, nhanNong;
    final Map<String, JTextField> oTs = new LinkedHashMap<>();
    String veCu;

    ServerGui(String host, int cong) {
        super("Máy chủ đặt vé xe khách MR6.14", "Tài nguyên: chỗ × chặng (Sài Gòn – Bảo Lộc – Đà Lạt – Nha Trang)",
                bao -> new MayChu.MayChuDatVeXe("khoa", 2.0, bao, Json.obj()), host, cong, 500, 200, MoPhong.SO_LAN_MAC_DINH);
        xay();
    }

    MayChu.MayChuDatVeXe sv() { return (MayChu.MayChuDatVeXe) lop; }

    @Override
    protected void veTaiNguyen(JPanel g) {
        JPanel f = GiaoDienChung.hang();
        cv = new GiaoDienChung.KhungVe(184, 212, gr -> {
            gr.setColor(DatVeXeGui.NEN_XE);
            gr.fillRect(0, 0, 184, 212);
            DatVeXeGui.SoDoXe s = soDo;
            Map<String, Object> t = ttVe;
            if (s == null || t == null) return;
            s.veNen(gr);
            String nong = String.valueOf(t.get("ghe_nong"));
            s.ve(gr, t, null, 0, MayChu.SO_CHANG, null, nong.isEmpty() ? null : Map.of(nong, new Color(0xff3030)));
        });
        f.add(cv);
        JPanel t = GiaoDienChung.cot();
        nhanXe = GiaoDienChung.nhan(" ", GiaoDienChung.chu(12, true), null);
        t.add(nhanXe);
        Object[][] loai = {{"trong", "Trống", Color.WHITE}, {"giu", "Đang giữ", DatVeXeGui.KHAC_GIU}, {"ban", "Đã bán", DatVeXeGui.DA_BAN}};
        for (Object[] x : loai) {
            JLabel o = new JLabel();
            o.setOpaque(true);
            o.setBackground((Color) x[2]);
            o.setBorder(BorderFactory.createLineBorder(Color.BLACK));
            o.setPreferredSize(new Dimension(16, 16));
            JLabel l = GiaoDienChung.nhan(" " + x[1] + ": –", GiaoDienChung.chu(13, true), null);
            nhanDem.put((String) x[0], l);
            t.add(GiaoDienChung.hang(o, l));
        }
        nhanChang = GiaoDienChung.nhan(" ", GiaoDienChung.chuDeu(11, false), null);
        nhanNong = GiaoDienChung.nhan(" ", GiaoDienChung.chu(11, false), null);
        t.add(Box.createVerticalStrut(4));
        t.add(nhanChang);
        t.add(Box.createVerticalStrut(4));
        t.add(nhanNong);
        f.add(Box.createHorizontalStrut(6));
        f.add(t);
        g.add(f);
        JPanel p = new JPanel(new GridBagLayout());
        p.setBackground(GiaoDienChung.NEN);
        p.setAlignmentX(0);
        Object[][] o = {{"ttl_s", "Giữ chỗ (s)", 4}, {"loai_xe", "Loại xe", 5}, {"ghe_nong", "Ghế nóng", 3}};
        for (int i = 0; i < o.length; i++) {
            String ma = (String) o[i][0];
            Object v = lop.thamSo.get(ma);
            p.add(GiaoDienChung.nhan(o[i][1] + ":"), GiaoDienChung.o(i * 2, 0));
            JTextField x = GiaoDienChung.oNhap(v instanceof Double d ? Chung.g(d) : v, (int) o[i][2]);
            oTs.put(ma, x);
            p.add(x, GiaoDienChung.o(i * 2 + 1, 0));
        }
        g.add(Box.createVerticalStrut(4));
        g.add(p);
    }

    @Override
    protected Map<String, Object> thamSoDatLai() {
        double ttl;
        try {
            ttl = Double.parseDouble(oTs.get("ttl_s").getText().strip());
        } catch (NumberFormatException e) {
            throw new IllegalArgumentException("thời hạn giữ chỗ phải là số");
        }
        String loai = oTs.get("loai_xe").getText().strip().toLowerCase();
        Map<String, Object> ts = Json.obj("ttl_s", ttl, "loai_xe", loai);
        String nong = oTs.get("ghe_nong").getText().strip().toUpperCase();
        if (!(!loai.equals(sv().tsS("loai_xe")) && nong.equals(sv().gheNong())))   // đổi loại xe mà không sửa ghế nóng: máy chủ tự chọn chỗ đầu
            ts.put("ghe_nong", nong);
        return ts;
    }

    /** Điền ô tham số (kịch bản demo) – như người dùng gõ. */
    void datThamSo(Map<String, Object> ts) { ts.forEach((k, v) -> oTs.get(k).setText(String.valueOf(v))); }

    @Override
    protected void capNhatTaiNguyen(Map<String, Object> tt, Map<String, Object> tk) {
        Map<String, Object> dem = Chung.map(tt.get("dem"));
        String[][] ten = {{"trong", "Trống"}, {"giu", "Đang giữ"}, {"ban", "Đã bán"}};
        for (String[] x : ten) nhanDem.get(x[0]).setText(" " + x[1] + ": " + dem.get(x[0]));
        nhanXe.setText(tt.get("ten_xe") + ", chuyến " + tt.get("chuyen"));
        List<Object> ghe = Chung.list(tt.get("ghe")), sd = Chung.list(tt.get("so_do"));
        List<String> dong = new ArrayList<>();
        for (int k = 0; k < MayChu.SO_CHANG; k++) {
            int ban = 0;
            for (Object d : sd) if (String.valueOf(d).charAt(k) == 'b') ban++;
            dong.add(Chung.f("%s-%s %2d/%d", MayChu.MA_TRAM.get(k), MayChu.MA_TRAM.get(k + 1), ban, ghe.size()));
        }
        nhanChang.setText(GiaoDienChung.html("Đã bán theo chặng:\n" + String.join("\n", dong)));
        String nong = String.valueOf(tt.get("ghe_nong"));
        if (nong.isEmpty()) nhanNong.setText("Ghế nóng (mô phỏng): ngẫu nhiên");
        else {
            String d = DatVeXeGui.dongCua(tt, nong);
            Map<String, Object> chu = Chung.map(tt.get("chu"));
            List<String> p = new ArrayList<>();
            for (int k = 0; k < MayChu.SO_CHANG; k++)
                if (d.charAt(k) != '.') p.add(MayChu.MA_TRAM.get(k) + "-" + MayChu.MA_TRAM.get(k + 1) + ": " + chu.get(nong + "#" + k));
            nhanNong.setText(GiaoDienChung.html("Ghế nóng (mô phỏng): " + nong + "\n" + (p.isEmpty() ? "trống cả tuyến" : String.join(", ", p))));
        }
        String loai = String.valueOf(tt.get("loai_xe"));
        if (soDo == null || !soDo.loaiXe.equals(loai)) {
            soDo = new DatVeXeGui.SoDoXe(loai, 0.5, false);
            veCu = null;
            datThamSo(Json.obj("loai_xe", loai, "ghe_nong", nong));   // đổi loại xe: ô ghế nóng theo máy chủ
        }
        String khoa = String.valueOf(tt.get("so_do")) + nong;
        ttVe = tt;
        if (khoa.equals(veCu)) return;
        veCu = khoa;
        cv.repaint();
    }

    public static void main(String[] args) {
        GiaoDienChung.NhatKy.inRa = true;                            // chạy riêng: nhật ký cũng ra dòng lệnh
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int cong = args.length > 1 ? Integer.parseInt(args[1]) : 43782;
        SwingUtilities.invokeLater(() -> {
            GiaoDienChung.caiDat();
            new ServerGui(host, cong).frame.setVisible(true);
        });
    }
}
