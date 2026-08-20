//! Translated from `src/nvim/tui/ugrid.c` and `ugrid.h` in full.

/// One TUI grid cell (`UCell`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UCell {
    /// Screen character.
    pub data: crate::types_defs::ScharT,
    /// Highlight attributes.
    pub attr: crate::types_defs::SattrT,
}

/// TUI shadow grid (`UGrid`).
#[derive(Debug, Default)]
pub struct UGrid {
    /// Current cursor row.
    pub row: i32,
    /// Current cursor column.
    pub col: i32,
    /// Grid width.
    pub width: i32,
    /// Grid height.
    pub height: i32,
    /// Row-major cells.
    pub cells: Vec<Vec<UCell>>,
}

/// Initialize an empty grid (`ugrid_init`).
pub fn ugrid_init(grid: &mut UGrid) {
    grid.cells.clear();
}

/// Release all grid cells (`ugrid_free`).
pub fn ugrid_free(grid: &mut UGrid) {
    destroy_cells(grid);
}

/// Replace the grid allocation with `width * height` zeroed cells
/// (`ugrid_resize`).
pub fn ugrid_resize(grid: &mut UGrid, width: i32, height: i32) {
    destroy_cells(grid);
    let width = usize::try_from(width).expect("grid width is nonnegative");
    let height =
        usize::try_from(height).expect("grid height is nonnegative");
    grid.cells = vec![vec![UCell::default(); width]; height];
    grid.width = width as i32;
    grid.height = height as i32;
}

/// Clear the whole grid to spaces with attribute zero (`ugrid_clear`).
pub fn ugrid_clear(grid: &mut UGrid) {
    clear_region(grid, 0, grid.height - 1, 0, grid.width - 1, 0);
}

/// Clear `[col, endcol)` in one row (`ugrid_clear_chunk`).
pub fn ugrid_clear_chunk(
    grid: &mut UGrid,
    row: i32,
    col: i32,
    endcol: i32,
    attr: crate::types_defs::SattrT,
) {
    clear_region(grid, row, row, col, endcol - 1, attr);
}

/// Move the shadow-grid cursor (`ugrid_goto`).
pub fn ugrid_goto(grid: &mut UGrid, row: i32, col: i32) {
    grid.row = row;
    grid.col = col;
}

/// Scroll one rectangular region by `count` rows (`ugrid_scroll`).
pub fn ugrid_scroll(
    grid: &mut UGrid,
    top: i32,
    bot: i32,
    left: i32,
    right: i32,
    count: i32,
) {
    assert!(right >= left && left >= 0);
    let (mut row, stop, step) = if count > 0 {
        (top, bot - count + 1, 1)
    } else {
        (bot, top - count - 1, -1)
    };
    let left = usize::try_from(left).expect("left column is nonnegative");
    let right =
        usize::try_from(right).expect("right column is nonnegative");

    while row != stop {
        let source_row = usize::try_from(row + count)
            .expect("source row is nonnegative");
        let target_row =
            usize::try_from(row).expect("target row is nonnegative");
        let source = grid.cells[source_row][left..=right].to_vec();
        grid.cells[target_row][left..=right].copy_from_slice(&source);
        row += step;
    }
}

fn clear_region(
    grid: &mut UGrid,
    top: i32,
    bot: i32,
    left: i32,
    right: i32,
    attr: crate::types_defs::SattrT,
) {
    if top > bot || left > right {
        return;
    }
    let left = usize::try_from(left).expect("left column is nonnegative");
    let right =
        usize::try_from(right).expect("right column is nonnegative");
    for row in top..=bot {
        let row = usize::try_from(row).expect("row is nonnegative");
        for cell in &mut grid.cells[row][left..=right] {
            cell.data = crate::grid::schar_from_ascii(b' ');
            cell.attr = attr;
        }
    }
}

fn destroy_cells(grid: &mut UGrid) {
    grid.cells.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbered_grid() -> UGrid {
        let mut grid = UGrid::default();
        ugrid_resize(&mut grid, 3, 4);
        for (row, cells) in grid.cells.iter_mut().enumerate() {
            for cell in cells {
                cell.data = row as u32;
            }
        }
        grid
    }

    #[test]
    fn init_and_free_release_cells_without_resetting_dimensions() {
        let mut grid = UGrid::default();
        ugrid_resize(&mut grid, 3, 2);
        ugrid_init(&mut grid);
        assert!(grid.cells.is_empty());
        assert_eq!((grid.width, grid.height), (3, 2));

        ugrid_resize(&mut grid, 2, 1);
        ugrid_free(&mut grid);
        assert!(grid.cells.is_empty());
        assert_eq!((grid.width, grid.height), (2, 1));
    }

    #[test]
    fn resize_allocates_zeroed_cells_and_clear_writes_spaces() {
        let mut grid = UGrid::default();
        ugrid_resize(&mut grid, 3, 2);
        assert_eq!(grid.cells, vec![vec![UCell::default(); 3]; 2]);

        ugrid_clear(&mut grid);
        assert!(grid.cells.iter().flatten().all(|cell| {
            cell.data == crate::grid::schar_from_ascii(b' ')
                && cell.attr == 0
        }));
    }

    #[test]
    fn clear_chunk_and_goto_update_only_the_requested_state() {
        let mut grid = numbered_grid();
        ugrid_clear_chunk(&mut grid, 1, 1, 3, 42);
        assert_eq!(grid.cells[1][0].data, 1);
        assert_eq!(
            &grid.cells[1][1..],
            &[
                UCell {
                    data: crate::grid::schar_from_ascii(b' '),
                    attr: 42,
                },
                UCell {
                    data: crate::grid::schar_from_ascii(b' '),
                    attr: 42,
                },
            ]
        );

        ugrid_goto(&mut grid, 2, 1);
        assert_eq!((grid.row, grid.col), (2, 1));
    }

    #[test]
    fn scroll_positive_copies_lower_rows_upward() {
        let mut grid = numbered_grid();
        ugrid_scroll(&mut grid, 0, 3, 0, 2, 1);
        assert_eq!(
            grid.cells
                .iter()
                .map(|row| row[0].data)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 3]
        );
    }

    #[test]
    fn scroll_negative_copies_upper_rows_downward() {
        let mut grid = numbered_grid();
        ugrid_scroll(&mut grid, 0, 3, 0, 2, -1);
        assert_eq!(
            grid.cells
                .iter()
                .map(|row| row[0].data)
                .collect::<Vec<_>>(),
            vec![0, 0, 1, 2]
        );
    }
}
