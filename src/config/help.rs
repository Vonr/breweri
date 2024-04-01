use std::process::exit;

pub fn print_help() {
    println!(
        r#"Usage: breweri [OPTION]... QUERY
Search for QUERY in Homebrew repositories,
Example:
   breweri rustup

Options:
   -h
       Print this help and exit
Keybinds:
    Both:
       <Escape>
           Switch Modes
       <C-c>
           Exit parui
   Insert:
       <Return>
           Search for query
       <C-w>
           Remove previous word
   Select:
       i, /
           Enter insert mode
       <Return>
           Install selected packages
       <C-j>, <C-Down>
           Move info one row down
       <C-k>, <C-Up>
           Move info one row up
       h, <Left>, <PgUp>
           Move one page back
       j, <Down>
           Move one row down
       k, <Up>
           Move one row up
       l, <Right>, <PgDn>
           Move one page forwards
       g, <Home>
           Go to start
       G, <End>
           Go to end
       <Space>
           Select/deselect package
       c
           Clear selections
       <S-R>
           Remove selected packages
       q
           Exit breweri"#
    );
    exit(0);
}
