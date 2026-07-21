#[cfg(test)]
mod tests {
    use crate::{
        data::{ClassId, GearId, SkillId},
        world::Game,
    };

    #[test]
    fn first_baseline_moves_match_apply_phase_checkpoints() {
        let mut game = Game::start(1_701_033, ClassId::Agent);
        game.rng.set_state(446_477_789);
        game.command('y');
        assert_eq!(
            (game.turns, game.player.cell, game.rng.state()),
            (1, 1_458, 446_477_789)
        );
        game.rng.set_state(1_310_547_126);
        game.command('h');
        assert_eq!(
            (game.turns, game.player.cell, game.rng.state()),
            (2, 1_457, 1_310_547_126)
        );
        game.rng.set_state(843_971_397);
        game.command('h');
        assert_eq!(
            (game.turns, game.player.cell, game.rng.state()),
            (3, 1_456, 843_971_397)
        );
        assert!(
            game.player
                .inventory
                .iter()
                .any(|item| item.gear == crate::data::GearId::BattlePlate)
        );
    }

    #[test]
    fn backup_command_spawns_temporary_friendly_agents() {
        let mut game = Game::start(1_701_033, ClassId::Agent);
        game.player.skills |= SkillId::Backup.bit();
        game.command('B');
        assert_eq!(game.turns, 1);
        assert_eq!(game.player.backup_cooldown, 79);
        assert!(game.mobs.iter().any(|mob| {
            mob.friendly && mob.hp > 0 && mob.life == 59 && mob.damage_override == Some([3, 8])
        }));
    }

    #[test]
    fn shop_purchase_uses_selected_uid_when_names_repeat() {
        let mut game = Game::start(1_701_094, ClassId::Tech);
        game.player.credits = 50;
        let mut expensive = game.make_item(GearId::NoisyCricket, 0);
        expensive.price = 100;
        let mut affordable = game.make_item(GearId::NoisyCricket, 0);
        affordable.price = 40;
        let affordable_uid = affordable.uid;
        game.shop_room = Some(0);
        game.rooms[0].stock = vec![expensive, affordable];

        assert!(game.buy_uid(affordable_uid));
        assert_eq!(game.player.credits, 10);
        assert!(
            game.player
                .inventory
                .iter()
                .any(|item| item.uid == affordable_uid)
        );
        assert_eq!(game.rooms[0].stock.len(), 1);
    }

    #[test]
    fn universal_remote_command_toggles_polymorph() {
        let mut game = Game::start(1_701_033, ClassId::Agent);
        game.player.skills |= SkillId::UniversalRemote.bit();
        game.command('P');
        assert_eq!(game.turns, 1);
        assert!(game.player.poly_form.is_some());
        assert_eq!(game.player.poly_turns, 998);
        game.command('P');
        assert_eq!(game.turns, 2);
        assert!(game.player.poly_form.is_none());
    }
}
