export function moveItemAt<T>(items: readonly T[], from: number, to: number): T[] {
  if (
    from === to ||
    from < 0 ||
    to < 0 ||
    from >= items.length ||
    to >= items.length
  ) {
    return [...items];
  }
  const next = [...items];
  const [moved] = next.splice(from, 1);
  next.splice(to, 0, moved!);
  return next;
}

/** Viewport Y of the floating card's center, used for drop-target hit testing. */
export function slotHitY(
  pointerY: number,
  grabY: number,
  height: number,
): number {
  return pointerY - grabY + height / 2;
}

/** Index whose vertical midpoint is nearest to `clientY`, for list drag-over. */
export function slotIndexFromClientY(
  slots: Array<{ top: number; bottom: number }>,
  clientY: number,
): number | null {
  if (!slots.length) return null;
  for (let i = 0; i < slots.length; i += 1) {
    const slot = slots[i]!;
    const mid = (slot.top + slot.bottom) / 2;
    if (clientY < mid) return i;
  }
  return slots.length - 1;
}

/** Pixel shift so a list item previews a drop without committing order. */
export function slotShiftY(
  index: number,
  dragIndex: number,
  overIndex: number,
  stride: number,
): number {
  if (dragIndex === overIndex || stride === 0) return 0;
  if (index === dragIndex) return (overIndex - dragIndex) * stride;
  if (dragIndex < overIndex && index > dragIndex && index <= overIndex) {
    return -stride;
  }
  if (dragIndex > overIndex && index >= overIndex && index < dragIndex) {
    return stride;
  }
  return 0;
}
