mod elements;
use elements::*;
use std::cell::UnsafeCell;


struct Player
{
    inventory : Vec::<Pawn>,
    color : PlayerColor
}

impl Player
{
    fn new(color : PlayerColor,board:UnsafeCell<Board>) -> Player
    {
        let mut inventory = Vec::<Pawn>::with_capacity(16);
        for i in 0..8 {
            inventory.push(Pawn::new(color,i,board.clone()));
            board.get_mut().arr[inventory[i as usize].position.0][inventory[i as usize].position.1] = Some(Box::new(inventory[i as usize]));
        }
        Player{inventory:inventory, color:color}
    }
}


pub struct Board
{
    arr : [[Option::<Box<Pawn>>;8];8],
    players : Option<(Player,Player)>,
    turn : bool
}

impl Board
{
    pub fn new() -> Board
    {
        let mut nonarr : [[Option::<UnsafeCell<Pawn>>;8];8] =  [[None;8];8];
        for i in 0..8 { for j in 0..8 {nonarr[i][j]=None;}}
        let mut B = Board{arr:nonarr, players :None, turn : true};
        let white = Player::new(PlayerColor::WHITE, UnsafeCell::new(B));
        let black = Player::new(PlayerColor::BLACK, UnsafeCell::new(B));
        B.players = Some((white,black));

        B
    }

    pub fn show(&self) ->()
    {
        for i in 0..8
        {
            let mut Line  = Vec::<&str>::with_capacity(8);
            for j in 0..8
            {
                match self.arr[i][j]
                {
                    None =>
                    {
                        if ( (i%2==0) ^ (j%2 ==0)) {Line.push(BLK_SQUARE);}
                        else{Line.push(WHT_SQUARE);}
                    }
                    Some(x) => {Line.push(x.getch());}
                }
            }

            println!("{}",Line.into_iter().collect::<String>());
        }
    }
}



