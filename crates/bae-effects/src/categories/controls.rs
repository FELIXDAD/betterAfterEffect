mod expression;
use crate::Registry;

pub(crate) fn register(registry: &mut Registry) {
    registry.effects.extend(expression::builtins());
}
