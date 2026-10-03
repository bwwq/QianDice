import 'dart:ui' show ImageFilter;
import 'package:fluent_ui/fluent_ui.dart';

/// Shared surface roles keep the desktop pages and controls visually consistent.
class WorkspacePalette {
  const WorkspacePalette(this.dark);
  final bool dark;
  static WorkspacePalette of(BuildContext context) =>
      WorkspacePalette(FluentTheme.of(context).brightness == Brightness.dark);

  Color get canvas => dark ? const Color(0xff101829) : const Color(0xfff4f6fc);
  Color get section => dark ? const Color(0xe6202b40) : const Color(0xe6ffffff);
  Color get control => dark ? const Color(0xff29364d) : const Color(0xffedf1fa);
  Color get text => dark ? const Color(0xffecf1ff) : const Color(0xff25304b);
  Color get muted => dark ? const Color(0xffa1aec7) : const Color(0xff66728a);
  Color get selected =>
      dark ? const Color(0xff344366) : const Color(0xffe8edff);
  Color get accent => dark ? const Color(0xffa9b7ff) : const Color(0xff5868ce);
  Color get edge => dark ? const Color(0x18c0d1ff) : const Color(0xafffffff);
  Color get success => dark ? const Color(0xff74dbc0) : const Color(0xff289d86);
  List<Color> get backdrop => dark
      ? const [Color(0xff182944), Color(0xff101829), Color(0xff1e2440)]
      : const [Color(0xffe5ecfb), Color(0xfff6f8fc), Color(0xffeeebfc)];
}

class DiceMark extends StatelessWidget {
  const DiceMark({super.key, this.size = 44});
  final double size;
  @override
  Widget build(BuildContext context) => Container(
    width: size,
    height: size,
    decoration: BoxDecoration(
      gradient: const LinearGradient(
        begin: Alignment.topLeft,
        end: Alignment.bottomRight,
        colors: [Color(0xff8fa9f5), Color(0xff6b70da)],
      ),
      borderRadius: BorderRadius.circular(size * .32),
      boxShadow: [
        BoxShadow(
          color: const Color(0xff7889e5).withValues(alpha: .18),
          blurRadius: 20,
          offset: const Offset(0, 6),
        ),
      ],
    ),
    child: Icon(FluentIcons.cube_shape, size: size * .52, color: Colors.white),
  );
}

class StatusPill extends StatelessWidget {
  const StatusPill({super.key, required this.label});
  final String label;
  @override
  Widget build(BuildContext context) {
    final colors = WorkspacePalette.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      decoration: BoxDecoration(
        color: colors.success.withValues(alpha: .09),
        borderRadius: BorderRadius.circular(20),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Container(
            width: 6,
            height: 6,
            decoration: BoxDecoration(
              color: colors.success,
              shape: BoxShape.circle,
            ),
          ),
          const SizedBox(width: 8),
          Text(
            label,
            style: TextStyle(
              fontSize: 12,
              color: colors.success,
              fontWeight: FontWeight.w500,
            ),
          ),
        ],
      ),
    );
  }
}

class DesktopShell extends StatelessWidget {
  const DesktopShell({
    super.key,
    required this.destinations,
    required this.selected,
    required this.onSelect,
    required this.expanded,
    required this.onToggle,
    required this.onTheme,
    required this.onRefresh,
    required this.busy,
    required this.child,
  });
  final List<(String, IconData)> destinations;
  final int selected;
  final ValueChanged<int> onSelect;
  final bool expanded, busy;
  final VoidCallback onToggle, onTheme, onRefresh;
  final Widget child;

  Widget navigation(BuildContext context, int index, bool open) {
    final colors = WorkspacePalette.of(context);
    final active = selected == index;
    final button = SizedBox(
      height: 44,
      width: double.infinity,
      child: Button(
        onPressed: () => onSelect(index),
        style: ButtonStyle(
          padding: const WidgetStatePropertyAll(
            EdgeInsets.symmetric(horizontal: 14),
          ),
          foregroundColor: WidgetStatePropertyAll(
            active ? colors.accent : colors.muted,
          ),
          backgroundColor: WidgetStateProperty.resolveWith(
            (states) => active
                ? colors.dark
                      ? colors.selected
                      : Colors.white.withValues(alpha: .9)
                : states.contains(WidgetState.hovered)
                ? colors.control.withValues(alpha: .6)
                : Colors.transparent,
          ),
          shape: WidgetStateProperty.resolveWith(
            (states) => RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(12),
              side: BorderSide(
                color: states.contains(WidgetState.focused)
                    ? colors.accent
                    : Colors.transparent,
              ),
            ),
          ),
        ),
        child: Row(
          mainAxisAlignment: open
              ? MainAxisAlignment.start
              : MainAxisAlignment.center,
          children: [
            Icon(destinations[index].$2, size: 18),
            if (open) ...[
              const SizedBox(width: 12),
              Flexible(
                child: Text(
                  destinations[index].$1,
                  style: TextStyle(
                    fontWeight: active ? FontWeight.w600 : FontWeight.w400,
                  ),
                ),
              ),
            ],
          ],
        ),
      ),
    );
    return Padding(
      padding: const EdgeInsets.only(bottom: 4),
      child: Semantics(
        selected: active,
        child: open
            ? button
            : Tooltip(message: destinations[index].$1, child: button),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = WorkspacePalette.of(context);
    return DecoratedBox(
      decoration: BoxDecoration(
        gradient: LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: colors.backdrop,
        ),
      ),
      child: LayoutBuilder(
        builder: (context, constraints) {
          final open = expanded && constraints.maxWidth >= 840;
          return Padding(
            padding: EdgeInsets.all(constraints.maxWidth < 600 ? 8 : 16),
            child: Row(
              children: [
                SizedBox(
                  width: open ? 212 : 72,
                  child: ClipRRect(
                    borderRadius: BorderRadius.circular(24),
                    child: BackdropFilter(
                      filter: ImageFilter.blur(sigmaX: 24, sigmaY: 24),
                      child: Container(
                        decoration: BoxDecoration(
                          color: colors.dark
                              ? const Color(0x901d2c45)
                              : const Color(0x90ffffff),
                          border: Border.all(color: colors.edge),
                          borderRadius: BorderRadius.circular(24),
                        ),
                        child: Column(
                          children: [
                            Padding(
                              padding: EdgeInsets.fromLTRB(
                                open ? 18 : 12,
                                24,
                                open ? 18 : 12,
                                20,
                              ),
                              child: Row(
                                mainAxisAlignment: MainAxisAlignment.center,
                                children: [
                                  const DiceMark(size: 42),
                                  if (open) ...[
                                    const SizedBox(width: 12),
                                    Expanded(
                                      child: Column(
                                        crossAxisAlignment:
                                            CrossAxisAlignment.start,
                                        children: [
                                          Text(
                                            '千变',
                                            style: TextStyle(
                                              fontSize: 23,
                                              fontWeight: FontWeight.w700,
                                              color: colors.text,
                                            ),
                                          ),
                                          Text(
                                            'QIANBIAN',
                                            style: TextStyle(
                                              fontSize: 9,
                                              letterSpacing: 2,
                                              color: colors.muted,
                                            ),
                                          ),
                                        ],
                                      ),
                                    ),
                                  ],
                                ],
                              ),
                            ),
                            Expanded(
                              child: ListView(
                                padding: const EdgeInsets.symmetric(
                                  horizontal: 10,
                                ),
                                children: [
                                  for (final group in [
                                    ('工作台', [0, 7]),
                                    ('跑团', [1, 2, 3, 4, 5]),
                                    ('管理', [6, 8]),
                                  ]) ...[
                                    if (open)
                                      Padding(
                                        padding: const EdgeInsets.fromLTRB(
                                          14,
                                          8,
                                          0,
                                          6,
                                        ),
                                        child: Text(
                                          group.$1,
                                          style: TextStyle(
                                            fontSize: 11,
                                            color: colors.muted,
                                          ),
                                        ),
                                      ),
                                    for (final index in group.$2)
                                      navigation(context, index, open),
                                    const SizedBox(height: 2),
                                  ],
                                ],
                              ),
                            ),
                            Padding(
                              padding: const EdgeInsets.all(12),
                              child: Row(
                                mainAxisAlignment: MainAxisAlignment.center,
                                children: [
                                  if (open) ...[
                                    Tooltip(
                                      message: '切换主题',
                                      child: IconButton(
                                        icon: const Icon(
                                          FluentIcons.brightness,
                                          size: 17,
                                        ),
                                        onPressed: onTheme,
                                      ),
                                    ),
                                    const Spacer(),
                                  ],
                                  Tooltip(
                                    message: open ? '收起侧栏' : '展开侧栏',
                                    child: IconButton(
                                      icon: const Icon(
                                        FluentIcons.global_nav_button,
                                        size: 17,
                                      ),
                                      onPressed: onToggle,
                                    ),
                                  ),
                                ],
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    children: [
                      Padding(
                        padding: const EdgeInsets.fromLTRB(20, 16, 20, 24),
                        child: Row(
                          children: [
                            Expanded(
                              child: Text(
                                destinations[selected].$1,
                                style: TextStyle(
                                  fontSize: 27,
                                  fontWeight: FontWeight.w600,
                                  color: colors.text,
                                ),
                              ),
                            ),
                            if (constraints.maxWidth >= 640)
                              const StatusPill(label: '后台运行中'),
                            const SizedBox(width: 10),
                            Tooltip(
                              message: '刷新',
                              child: IconButton(
                                onPressed: busy ? null : onRefresh,
                                icon: busy
                                    ? const SizedBox.square(
                                        dimension: 16,
                                        child: ProgressRing(strokeWidth: 2),
                                      )
                                    : const Icon(FluentIcons.refresh, size: 17),
                              ),
                            ),
                            if (!open)
                              Tooltip(
                                message: '切换主题',
                                child: IconButton(
                                  icon: const Icon(
                                    FluentIcons.brightness,
                                    size: 17,
                                  ),
                                  onPressed: onTheme,
                                ),
                              ),
                          ],
                        ),
                      ),
                      Expanded(child: child),
                    ],
                  ),
                ),
              ],
            ),
          );
        },
      ),
    );
  }
}

class Surface extends StatelessWidget {
  const Surface({
    super.key,
    required this.child,
    this.padding = const EdgeInsets.all(16),
  });
  final Widget child;
  final EdgeInsetsGeometry padding;

  @override
  Widget build(BuildContext context) => Container(
    padding: padding,
    decoration: BoxDecoration(
      color: WorkspacePalette.of(context).section,
      borderRadius: BorderRadius.circular(20),
      border: Border.all(color: WorkspacePalette.of(context).edge),
      boxShadow: [
        BoxShadow(
          color: Colors.black.withValues(alpha: .025),
          blurRadius: 24,
          offset: const Offset(0, 6),
        ),
      ],
    ),
    child: child,
  );
}

FluentThemeData workspaceTheme(Brightness brightness) {
  final colors = WorkspacePalette(brightness == Brightness.dark);
  final base = Typography.fromBrightness(brightness: brightness);
  final shape = WidgetStatePropertyAll<OutlinedBorder>(
    RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
  );
  return FluentThemeData(
    brightness: brightness,
    fontFamily: 'QianbianSans',
    accentColor: colors.accent.toAccentColor(),
    activeColor: colors.dark ? colors.canvas : Colors.white,
    inactiveColor: colors.text,
    scaffoldBackgroundColor: colors.canvas,
    acrylicBackgroundColor: colors.canvas,
    micaBackgroundColor: colors.canvas,
    cardColor: colors.section,
    menuColor: colors.control,
    typography: Typography.raw(
      title: base.title!.copyWith(
        fontSize: 28,
        height: 1.4,
        color: colors.text,
      ),
      subtitle: base.subtitle!.copyWith(fontSize: 16, color: colors.text),
      body: base.body!.copyWith(fontSize: 14, color: colors.text),
      caption: base.caption!.copyWith(fontSize: 13, color: colors.muted),
    ),
    buttonTheme: ButtonThemeData(
      defaultButtonStyle: ButtonStyle(
        shape: shape,
        padding: const WidgetStatePropertyAll(
          EdgeInsets.symmetric(horizontal: 16, vertical: 12),
        ),
        foregroundColor: WidgetStateProperty.resolveWith((states) => states.contains(WidgetState.disabled) ? colors.muted : colors.text),
        backgroundColor: WidgetStateProperty.resolveWith(
          (states) =>
              states.contains(WidgetState.hovered) ||
                  states.contains(WidgetState.pressed)
              ? colors.control
              : colors.section,
        ),
      ),
      filledButtonStyle: ButtonStyle(
        shape: shape,
        padding: const WidgetStatePropertyAll(
          EdgeInsets.symmetric(horizontal: 20, vertical: 12),
        ),
        backgroundColor: WidgetStateProperty.resolveWith(
          (states) => states.contains(WidgetState.disabled)
              ? colors.control
              : states.contains(WidgetState.hovered)
              ? Color.lerp(colors.accent, colors.text, .08)
              : colors.accent,
        ),
        foregroundColor: WidgetStateProperty.resolveWith((states) => states.contains(WidgetState.disabled)
            ? colors.muted : colors.dark ? const Color(0xff16213a) : Colors.white),
      ),
    ),
    navigationPaneTheme: NavigationPaneThemeData(
      backgroundColor: colors.canvas,
      overlayBackgroundColor: colors.section,
      highlightColor: colors.accent,
      tileColor: WidgetStateProperty.resolveWith(
        (states) => states.contains(WidgetState.selected)
            ? colors.selected
            : states.contains(WidgetState.hovered)
            ? colors.control
            : Colors.transparent,
      ),
      selectedTextStyle: WidgetStatePropertyAll(
        TextStyle(
          fontSize: 14,
          fontWeight: FontWeight.w600,
          color: colors.text,
        ),
      ),
      unselectedTextStyle: WidgetStatePropertyAll(
        TextStyle(fontSize: 14, color: colors.muted),
      ),
      selectedIconColor: WidgetStatePropertyAll(colors.text),
      unselectedIconColor: WidgetStatePropertyAll(colors.muted),
    ),
  );
}
