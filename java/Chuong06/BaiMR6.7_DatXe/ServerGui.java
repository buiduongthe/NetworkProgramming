import java.awt.Color;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import javax.swing.BorderFactory;
import javax.swing.Box;
import javax.swing.DefaultListCellRenderer;
import javax.swing.DefaultListModel;
import javax.swing.JComponent;
import javax.swing.JLabel;
import javax.swing.JList;
import javax.swing.JPanel;
import javax.swing.JScrollPane;
import javax.swing.JTextField;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.7 – Bảng điều khiển máy chủ đặt xe (Swing).
 *
 * Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký)
 * ở GiaoDienChung.CuaSoServerCoSo; tệp này chỉ vẽ khung "Tài nguyên": bản đồ thu nhỏ với các xe (xe đã có khách mờ
 * đi, ghi tên khách), số xe trống, danh sách xe và tham số đặt lại (số xe).
 *
 * Chạy:  java -cp out ServerGui [host] [cổng]        (mặc định 127.0.0.1 43712; 1 xe)
 */
public class ServerGui extends GiaoDienChung.CuaSoServerCoSo {
    static final int RONG = 200, CAO = 150;
    DatXeGui.BanDo banDo;
    JComponent cv;
    JLabel nhanTrong;
    JList<Object[]> ls;
    JTextField oSoXe;
    volatile List<Object> dsXe = List.of();
    String veCu;

    ServerGui(String host, int cong) {
        super("Máy chủ đặt xe MR6.7", "Tài nguyên: các xe (mỗi xe 1 chỗ)",
                bao -> new MayChu.MayChuDatXe("khoa", 2.0, bao, Json.obj("so_xe", 1L)), host, cong, 500, 200,
                MoPhong.SO_LAN_MAC_DINH);
        xay();
    }

    @Override
    protected void veTaiNguyen(JPanel g) {
        JPanel f = GiaoDienChung.hang();
        banDo = new DatXeGui.BanDo(RONG, CAO, true);
        cv = new GiaoDienChung.KhungVe(RONG, CAO, gr -> {
            banDo.veNen(gr);
            for (Object o : dsXe) {
                Map<String, Object> x = Chung.map(o);
                Object khach = x.get("khach");
                banDo.xe(gr, MayChu.diemCua(x.get("vi_tri")), DatXeGui.mauXe(x.get("mau")),
                        String.valueOf(khach != null ? khach : x.get("tai_xe")), true, khach != null, false);
            }
        });
        cv.setBorder(BorderFactory.createLineBorder(new Color(0x808080)));
        f.add(cv);
        JPanel t = GiaoDienChung.cot();
        nhanTrong = GiaoDienChung.nhan("–", GiaoDienChung.chu(21, true), null);
        t.add(nhanTrong);
        ls = new JList<>(new DefaultListModel<>());
        ls.setFont(GiaoDienChung.chuDeu(10, false));
        ls.setVisibleRowCount(6);
        ls.setFixedCellWidth(200);
        ls.setFocusable(false);
        ls.setCellRenderer(new DefaultListCellRenderer() {
            @Override
            public java.awt.Component getListCellRendererComponent(JList<?> l, Object v, int i, boolean c, boolean td) {
                Object[] x = (Object[]) v;
                super.getListCellRendererComponent(l, x[0], i, false, false);
                setForeground((boolean) x[1] ? new Color(0xa00000) : Color.BLACK);
                return this;
            }
        });
        JScrollPane cuon = new JScrollPane(ls);
        cuon.setAlignmentX(0);
        t.add(Box.createVerticalStrut(2));
        t.add(cuon);
        f.add(Box.createHorizontalStrut(6));
        f.add(t);
        g.add(f);
        oSoXe = GiaoDienChung.oNhap(lop.thamSo.get("so_xe"), 4);
        g.add(Box.createVerticalStrut(4));
        g.add(GiaoDienChung.hang(GiaoDienChung.nhan("Số xe (1–50):"), oSoXe));
    }

    @Override
    protected Map<String, Object> thamSoDatLai() {
        try {
            return Json.obj("so_xe", Long.parseLong(oSoXe.getText().strip()));
        } catch (NumberFormatException e) {
            throw new IllegalArgumentException("số xe phải là số nguyên");
        }
    }

    /** Điền ô tham số (kịch bản demo) – như người dùng gõ. */
    void datThamSo(Map<String, Object> ts) { if (ts.containsKey("so_xe")) oSoXe.setText(String.valueOf(ts.get("so_xe"))); }

    @Override
    protected void capNhatTaiNguyen(Map<String, Object> tt, Map<String, Object> tk) {
        long trong = Chung.so(tt.get("trong"));
        nhanTrong.setText("Trống " + trong + "/" + tt.get("so_xe") + " xe");
        nhanTrong.setForeground(trong > 0 ? GiaoDienChung.XANH_DAM : GiaoDienChung.DO_DAM);
        List<Object> xe = Chung.list(tt.get("xe"));
        StringBuilder khoaVe = new StringBuilder();
        for (Object o : xe) khoaVe.append(Chung.map(o).get("ma")).append('=').append(Chung.map(o).get("khach")).append(';');
        if (khoaVe.toString().equals(veCu)) return;
        veCu = khoaVe.toString();
        dsXe = new ArrayList<>(xe);
        cv.repaint();
        DefaultListModel<Object[]> m = (DefaultListModel<Object[]>) ls.getModel();
        m.clear();
        for (Object o : xe) {
            Map<String, Object> x = Chung.map(o);
            Object khach = x.get("khach");
            m.addElement(new Object[] {Chung.f("%2s %-6s%-11s%s", x.get("ma"), x.get("tai_xe"), x.get("bien_so"),
                    khach == null ? "·" : khach), khach != null});
        }
    }

    public static void main(String[] args) {
        GiaoDienChung.NhatKy.inRa = true;                            // chạy riêng: nhật ký cũng ra dòng lệnh
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int cong = args.length > 1 ? Integer.parseInt(args[1]) : 43712;
        SwingUtilities.invokeLater(() -> {
            GiaoDienChung.caiDat();
            new ServerGui(host, cong).frame.setVisible(true);
        });
    }
}
