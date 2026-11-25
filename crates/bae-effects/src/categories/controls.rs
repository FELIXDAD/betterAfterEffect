mod expression;
pub(crate) use expression::Control;
use crate::Registry;

pub(crate) fn register(registry: &mut Registry) {
    registry.effects.extend(expression::builtins());
}
