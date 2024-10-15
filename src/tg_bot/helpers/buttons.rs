use once_cell::sync::Lazy;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

// button (text, id)
pub type Button = (String, String);
pub type ButtonRow = Vec<Button>;
pub type ButtonMenu = Vec<ButtonRow>;
/// Creates a button keyboard from Button slice
pub fn make_keyboard(menu_items: &ButtonMenu) -> InlineKeyboardMarkup {
    let mut keyboard: Vec<Vec<InlineKeyboardButton>> = vec![];
    for row in menu_items {
        let keyboard_row = row
            .iter()
            .map(|button| InlineKeyboardButton::callback(button.0.to_owned(), button.1.to_owned()))
            .collect();
        keyboard.push(keyboard_row);
    }
    InlineKeyboardMarkup::new(keyboard)
}
