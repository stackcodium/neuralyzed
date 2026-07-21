use crate::{CELLS, HEIGHT, WIDTH, bitgrid::BitGrid, coordinates, index};

const UNSEEN: i16 = -2;
const ROOT: i16 = -1;
const DIRECTIONS: [(i8, i8); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

#[derive(Clone, Debug)]
pub struct NavigationGrid {
    pub walls: BitGrid,
    pub traps: BitGrid,
    pub visible_hostiles: BitGrid,
    pub poison: BitGrid,
}

impl Default for NavigationGrid {
    fn default() -> Self {
        Self {
            walls: BitGrid::EMPTY,
            traps: BitGrid::EMPTY,
            visible_hostiles: BitGrid::EMPTY,
            poison: BitGrid::EMPTY,
        }
    }
}

#[derive(Clone)]
pub struct Pathfinder {
    queue: [u16; CELLS],
    parents: [i16; CELLS],
    route: Vec<u16>,
}

impl Default for Pathfinder {
    fn default() -> Self {
        Self {
            queue: [0; CELLS],
            parents: [UNSEEN; CELLS],
            route: Vec::with_capacity(WIDTH + HEIGHT),
        }
    }
}

impl Pathfinder {
    pub fn best_nearest_target(
        &mut self,
        grid: &NavigationGrid,
        start: usize,
        extra_distance: usize,
        is_goal: impl Fn(usize) -> bool,
        score_goal: impl Fn(usize) -> i32,
    ) -> Option<usize> {
        self.parents.fill(UNSEEN);
        let mut head = 0;
        let mut tail = 1;
        let mut depth = 0_usize;
        let mut nearest_distance = usize::MAX;
        let mut best_target = None;
        let mut best_score = i32::MIN;
        self.queue[0] = start as u16;
        self.parents[start] = ROOT;

        while head < tail && depth <= nearest_distance.saturating_add(extra_distance) {
            let layer_end = tail;
            while head < layer_end {
                let current = self.queue[head] as usize;
                head += 1;
                if is_goal(current) {
                    if depth < nearest_distance {
                        nearest_distance = depth;
                        best_target = None;
                        best_score = i32::MIN;
                    }
                    let score = score_goal(current) - depth as i32 * 8;
                    if depth <= nearest_distance.saturating_add(extra_distance)
                        && score > best_score
                    {
                        best_target = Some(current);
                        best_score = score;
                    }
                    continue;
                }
                if depth >= nearest_distance.saturating_add(extra_distance) {
                    continue;
                }
                let (x, y) = coordinates(current);
                for (dx, dy) in DIRECTIONS {
                    let nx = x as isize + dx as isize;
                    let ny = y as isize + dy as isize;
                    if nx < 0 || ny < 0 || nx >= WIDTH as isize || ny >= HEIGHT as isize {
                        continue;
                    }
                    let next = index(nx as usize, ny as usize);
                    if self.parents[next] != UNSEEN
                        || grid.walls.contains(next)
                        || grid.traps.contains(next)
                    {
                        continue;
                    }
                    let goal = is_goal(next);
                    if !goal && (grid.visible_hostiles.contains(next) || grid.poison.contains(next))
                    {
                        continue;
                    }
                    self.parents[next] = current as i16;
                    self.queue[tail] = next as u16;
                    tail += 1;
                }
            }
            depth += 1;
        }
        best_target
    }

    pub fn shortest_straight<'a>(
        &'a mut self,
        grid: &NavigationGrid,
        start: usize,
        target: usize,
        avoid_poison: bool,
        allow_traps: bool,
    ) -> Option<&'a [u16]> {
        self.shortest(grid, start, target, avoid_poison, allow_traps, true)
    }

    pub fn shortest_fixed<'a>(
        &'a mut self,
        grid: &NavigationGrid,
        start: usize,
        target: usize,
        avoid_poison: bool,
        allow_traps: bool,
    ) -> Option<&'a [u16]> {
        self.shortest(grid, start, target, avoid_poison, allow_traps, false)
    }

    fn shortest<'a>(
        &'a mut self,
        grid: &NavigationGrid,
        start: usize,
        target: usize,
        avoid_poison: bool,
        allow_traps: bool,
        prefer_straight: bool,
    ) -> Option<&'a [u16]> {
        self.parents.fill(UNSEEN);
        self.route.clear();
        let mut head = 0;
        let mut tail = 1;
        self.queue[0] = start as u16;
        self.parents[start] = ROOT;
        let (start_x, start_y) = coordinates(start);
        let (target_x, target_y) = coordinates(target);
        let mut order = [0_u8, 1, 2, 3, 4, 5, 6, 7];
        let mut scores = [(0_u16, 0_u16, 0_u16); 8];

        while head < tail {
            let current = self.queue[head] as usize;
            head += 1;
            if current == target {
                let mut cursor = current as i16;
                while cursor >= 0 {
                    self.route.push(cursor as u16);
                    cursor = self.parents[cursor as usize];
                }
                self.route.reverse();
                return Some(&self.route);
            }
            let (x, y) = coordinates(current);
            if prefer_straight {
                order_directions(
                    &mut order,
                    &mut scores,
                    (start_x, start_y),
                    (target_x, target_y),
                    (x, y),
                );
            }
            for direction_index in order {
                let (dx, dy) = DIRECTIONS[direction_index as usize];
                let nx = x as isize + dx as isize;
                let ny = y as isize + dy as isize;
                if nx < 0 || ny < 0 || nx >= WIDTH as isize || ny >= HEIGHT as isize {
                    continue;
                }
                let next = index(nx as usize, ny as usize);
                if self.parents[next] != UNSEEN || grid.walls.contains(next) {
                    continue;
                }
                let goal = next == target;
                if !allow_traps && grid.traps.contains(next) {
                    continue;
                }
                if !goal
                    && (grid.visible_hostiles.contains(next)
                        || avoid_poison && grid.poison.contains(next))
                {
                    continue;
                }
                self.parents[next] = current as i16;
                self.queue[tail] = next as u16;
                tail += 1;
            }
        }
        None
    }
}

#[inline]
fn order_directions(
    order: &mut [u8; 8],
    scores: &mut [(u16, u16, u16); 8],
    start: (usize, usize),
    target: (usize, usize),
    current: (usize, usize),
) {
    let (start_x, start_y) = start;
    let (target_x, target_y) = target;
    let (x, y) = current;
    let line_dx = target_x as isize - start_x as isize;
    let line_dy = target_y as isize - start_y as isize;
    for i in 0..8 {
        order[i] = i as u8;
        let (dx, dy) = DIRECTIONS[i];
        let nx = x as isize + dx as isize;
        let ny = y as isize + dy as isize;
        let remaining = (target_x as isize - nx)
            .abs()
            .max((target_y as isize - ny).abs()) as u16;
        let deviation = (line_dx * (ny - start_y as isize) - line_dy * (nx - start_x as isize))
            .unsigned_abs() as u16;
        let ex = target_x as isize - nx;
        let ey = target_y as isize - ny;
        scores[i] = (remaining, deviation, (ex * ex + ey * ey) as u16);
    }
    for i in 1..8 {
        let direction = order[i];
        let mut j = i;
        while j > 0 && scores[direction as usize] < scores[order[j - 1] as usize] {
            order[j] = order[j - 1];
            j -= 1;
        }
        order[j] = direction;
    }
}

#[cfg(test)]
mod tests {
    use super::{NavigationGrid, Pathfinder};
    use crate::index;

    #[test]
    fn open_horizontal_route_is_straight() {
        let mut pathfinder = Pathfinder::default();
        let grid = NavigationGrid::default();
        let path = pathfinder
            .shortest_straight(&grid, index(6, 18), index(40, 18), false, true)
            .unwrap();
        assert_eq!(path.len(), 35);
        assert!(
            path.iter()
                .enumerate()
                .all(|(offset, cell)| *cell as usize == index(6 + offset, 18))
        );
    }

    #[test]
    fn obstacle_route_remains_shortest() {
        let mut pathfinder = Pathfinder::default();
        let mut grid = NavigationGrid::default();
        grid.walls.insert(index(8, 8));
        let path = pathfinder
            .shortest_straight(&grid, index(6, 8), index(10, 8), false, true)
            .unwrap();
        assert_eq!(path.len(), 5);
        assert!(!path.contains(&(index(8, 8) as u16)));
    }
}
